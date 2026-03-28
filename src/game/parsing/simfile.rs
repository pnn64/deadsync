use crate::engine::audio::decode;
use crate::game::{
    chart::{ChartData, GameplayChartData, StaminaCounts},
    note::NoteType,
    parsing::notes::ParsedNote,
    song::{SongBackgroundChange, SongBackgroundChangeTarget, SongData, get_song_cache},
    timing::{
        DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment, TimingData,
        TimingSegments, WarpSegment,
    },
};
use log::{debug, info, warn};
use rssp::parse::{decode_bytes, extract_bgchanges_values, unescape_tag};
use rssp::patterns::{PatternVariant, compute_box_counts, count_pattern};
use rssp::{AnalysisOptions, analyze};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bincode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::hash::Hasher;
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

mod cache;
mod scan;

pub(crate) use scan::collect_song_scan_roots;
pub use scan::{
    scan_and_load_songs, scan_and_load_songs_with_progress,
    scan_and_load_songs_with_progress_counts,
};

const SONG_ANALYSIS_MONO_THRESHOLD: usize = 6;
// Keep this at 1 throughout alpha, even if the serialized song payload changes.
// Old caches already fall back on decode/schema mismatch. Start bumping this only
// once we have a public beta/release and want explicit cache-version invalidation.
const SONG_CACHE_VERSION: u8 = 1;
const SONG_CACHE_MAGIC: [u8; 8] = *b"DSCACHE2";

// --- SERIALIZABLE MIRROR STRUCTS ---

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedArrowStats {
    pub total_arrows: u32,
    pub left: u32,
    pub down: u32,
    pub up: u32,
    pub right: u32,
    pub total_steps: u32,
    pub jumps: u32,
    pub hands: u32,
    pub mines: u32,
    pub holds: u32,
    pub rolls: u32,
    pub lifts: u32,
    pub fakes: u32,
    pub holding: i32,
}

impl From<&rssp::stats::ArrowStats> for CachedArrowStats {
    fn from(stats: &rssp::stats::ArrowStats) -> Self {
        Self {
            total_arrows: stats.total_arrows,
            left: stats.left,
            down: stats.down,
            up: stats.up,
            right: stats.right,
            total_steps: stats.total_steps,
            jumps: stats.jumps,
            hands: stats.hands,
            mines: stats.mines,
            holds: stats.holds,
            rolls: stats.rolls,
            lifts: stats.lifts,
            fakes: stats.fakes,
            holding: stats.holding,
        }
    }
}

impl From<CachedArrowStats> for rssp::stats::ArrowStats {
    fn from(stats: CachedArrowStats) -> Self {
        Self {
            total_arrows: stats.total_arrows,
            left: stats.left,
            down: stats.down,
            up: stats.up,
            right: stats.right,
            total_steps: stats.total_steps,
            jumps: stats.jumps,
            hands: stats.hands,
            mines: stats.mines,
            holds: stats.holds,
            rolls: stats.rolls,
            lifts: stats.lifts,
            fakes: stats.fakes,
            holding: stats.holding,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
struct CachedTechCounts {
    pub crossovers: u32,
    pub half_crossovers: u32,
    pub full_crossovers: u32,
    pub footswitches: u32,
    pub up_footswitches: u32,
    pub down_footswitches: u32,
    pub sideswitches: u32,
    pub jacks: u32,
    pub brackets: u32,
    pub doublesteps: u32,
}

impl From<&rssp::TechCounts> for CachedTechCounts {
    fn from(counts: &rssp::TechCounts) -> Self {
        Self {
            crossovers: counts.crossovers,
            half_crossovers: counts.half_crossovers,
            full_crossovers: counts.full_crossovers,
            footswitches: counts.footswitches,
            up_footswitches: counts.up_footswitches,
            down_footswitches: counts.down_footswitches,
            sideswitches: counts.sideswitches,
            jacks: counts.jacks,
            brackets: counts.brackets,
            doublesteps: counts.doublesteps,
        }
    }
}

impl From<CachedTechCounts> for rssp::TechCounts {
    fn from(counts: CachedTechCounts) -> Self {
        Self {
            crossovers: counts.crossovers,
            half_crossovers: counts.half_crossovers,
            full_crossovers: counts.full_crossovers,
            footswitches: counts.footswitches,
            up_footswitches: counts.up_footswitches,
            down_footswitches: counts.down_footswitches,
            sideswitches: counts.sideswitches,
            jacks: counts.jacks,
            brackets: counts.brackets,
            doublesteps: counts.doublesteps,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
struct CachedStaminaCounts {
    pub anchors: u32,
    pub triangles: u32,
    pub boxes: u32,
    pub towers: u32,
    pub doritos: u32,
    pub hip_breakers: u32,
    pub copters: u32,
    pub spirals: u32,
    pub candles: u32,
    pub candle_percent: f64,
    pub staircases: u32,
    pub mono: u32,
    pub mono_percent: f64,
    pub sweeps: u32,
}

impl From<&StaminaCounts> for CachedStaminaCounts {
    fn from(counts: &StaminaCounts) -> Self {
        Self {
            anchors: counts.anchors,
            triangles: counts.triangles,
            boxes: counts.boxes,
            towers: counts.towers,
            doritos: counts.doritos,
            hip_breakers: counts.hip_breakers,
            copters: counts.copters,
            spirals: counts.spirals,
            candles: counts.candles,
            candle_percent: counts.candle_percent,
            staircases: counts.staircases,
            mono: counts.mono,
            mono_percent: counts.mono_percent,
            sweeps: counts.sweeps,
        }
    }
}

impl From<CachedStaminaCounts> for StaminaCounts {
    fn from(counts: CachedStaminaCounts) -> Self {
        Self {
            anchors: counts.anchors,
            triangles: counts.triangles,
            boxes: counts.boxes,
            towers: counts.towers,
            doritos: counts.doritos,
            hip_breakers: counts.hip_breakers,
            copters: counts.copters,
            spirals: counts.spirals,
            candles: counts.candles,
            candle_percent: counts.candle_percent,
            staircases: counts.staircases,
            mono: counts.mono,
            mono_percent: counts.mono_percent,
            sweeps: counts.sweeps,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
enum CachedSpeedUnit {
    Beats,
    Seconds,
}

impl From<SpeedUnit> for CachedSpeedUnit {
    fn from(unit: SpeedUnit) -> Self {
        match unit {
            SpeedUnit::Beats => Self::Beats,
            SpeedUnit::Seconds => Self::Seconds,
        }
    }
}

impl From<CachedSpeedUnit> for SpeedUnit {
    fn from(unit: CachedSpeedUnit) -> Self {
        match unit {
            CachedSpeedUnit::Beats => Self::Beats,
            CachedSpeedUnit::Seconds => Self::Seconds,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
struct CachedSpeedSegment {
    beat: f32,
    ratio: f32,
    delay: f32,
    unit: CachedSpeedUnit,
}

impl From<&SpeedSegment> for CachedSpeedSegment {
    fn from(segment: &SpeedSegment) -> Self {
        Self {
            beat: segment.beat,
            ratio: segment.ratio,
            delay: segment.delay,
            unit: segment.unit.into(),
        }
    }
}

impl From<CachedSpeedSegment> for SpeedSegment {
    fn from(segment: CachedSpeedSegment) -> Self {
        Self {
            beat: segment.beat,
            ratio: segment.ratio,
            delay: segment.delay,
            unit: segment.unit.into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedTimingSegments {
    beat0_offset_adjust: f32,
    bpms: Vec<(f32, f32)>,
    stops: Vec<(f32, f32)>,
    delays: Vec<(f32, f32)>,
    warps: Vec<(f32, f32)>,
    speeds: Vec<CachedSpeedSegment>,
    scrolls: Vec<(f32, f32)>,
    fakes: Vec<(f32, f32)>,
}

impl From<&TimingSegments> for CachedTimingSegments {
    fn from(segments: &TimingSegments) -> Self {
        Self {
            beat0_offset_adjust: segments.beat0_offset_adjust,
            bpms: segments.bpms.clone(),
            stops: segments
                .stops
                .iter()
                .map(|seg| (seg.beat, seg.duration))
                .collect(),
            delays: segments
                .delays
                .iter()
                .map(|seg| (seg.beat, seg.duration))
                .collect(),
            warps: segments
                .warps
                .iter()
                .map(|seg| (seg.beat, seg.length))
                .collect(),
            speeds: segments
                .speeds
                .iter()
                .map(CachedSpeedSegment::from)
                .collect(),
            scrolls: segments
                .scrolls
                .iter()
                .map(|seg| (seg.beat, seg.ratio))
                .collect(),
            fakes: segments
                .fakes
                .iter()
                .map(|seg| (seg.beat, seg.length))
                .collect(),
        }
    }
}

impl From<CachedTimingSegments> for TimingSegments {
    fn from(segments: CachedTimingSegments) -> Self {
        Self {
            beat0_offset_adjust: segments.beat0_offset_adjust,
            bpms: segments.bpms,
            stops: segments
                .stops
                .into_iter()
                .map(|(beat, duration)| StopSegment { beat, duration })
                .collect(),
            delays: segments
                .delays
                .into_iter()
                .map(|(beat, duration)| DelaySegment { beat, duration })
                .collect(),
            warps: segments
                .warps
                .into_iter()
                .map(|(beat, length)| WarpSegment { beat, length })
                .collect(),
            speeds: segments
                .speeds
                .into_iter()
                .map(SpeedSegment::from)
                .collect(),
            scrolls: segments
                .scrolls
                .into_iter()
                .map(|(beat, ratio)| ScrollSegment { beat, ratio })
                .collect(),
            fakes: segments
                .fakes
                .into_iter()
                .map(|(beat, length)| FakeSegment { beat, length })
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
enum CachedNoteType {
    Tap,
    Hold,
    Roll,
    Mine,
    Lift,
    Fake,
}

impl From<NoteType> for CachedNoteType {
    fn from(note_type: NoteType) -> Self {
        match note_type {
            NoteType::Tap => Self::Tap,
            NoteType::Hold => Self::Hold,
            NoteType::Roll => Self::Roll,
            NoteType::Mine => Self::Mine,
            NoteType::Lift => Self::Lift,
            NoteType::Fake => Self::Fake,
        }
    }
}

impl From<CachedNoteType> for NoteType {
    fn from(note_type: CachedNoteType) -> Self {
        match note_type {
            CachedNoteType::Tap => Self::Tap,
            CachedNoteType::Hold => Self::Hold,
            CachedNoteType::Roll => Self::Roll,
            CachedNoteType::Mine => Self::Mine,
            CachedNoteType::Lift => Self::Lift,
            CachedNoteType::Fake => Self::Fake,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedParsedNote {
    row_index: u32,
    column: u8,
    note_type: CachedNoteType,
    tail_row_index: Option<u32>,
}

impl From<&ParsedNote> for CachedParsedNote {
    fn from(note: &ParsedNote) -> Self {
        Self {
            row_index: note.row_index as u32,
            column: note.column as u8,
            note_type: note.note_type.into(),
            tail_row_index: note.tail_row_index.map(|v| v as u32),
        }
    }
}

impl From<CachedParsedNote> for ParsedNote {
    fn from(note: CachedParsedNote) -> Self {
        Self {
            row_index: note.row_index as usize,
            column: note.column as usize,
            note_type: note.note_type.into(),
            tail_row_index: note.tail_row_index.map(|v| v as usize),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
enum CachedChartDisplayBpm {
    Specified { min: f64, max: f64 },
    Random,
}

impl From<CachedChartDisplayBpm> for crate::game::chart::ChartDisplayBpm {
    fn from(c: CachedChartDisplayBpm) -> Self {
        match c {
            CachedChartDisplayBpm::Specified { min, max } => Self::Specified { min, max },
            CachedChartDisplayBpm::Random => Self::Random,
        }
    }
}

impl From<&crate::game::chart::ChartDisplayBpm> for CachedChartDisplayBpm {
    fn from(c: &crate::game::chart::ChartDisplayBpm) -> Self {
        match c {
            crate::game::chart::ChartDisplayBpm::Specified { min, max } => {
                Self::Specified { min: *min, max: *max }
            }
            crate::game::chart::ChartDisplayBpm::Random => Self::Random,
        }
    }
}

fn parse_chart_display_bpm(tag: Option<&str>) -> Option<CachedChartDisplayBpm> {
    let s = tag?.trim();
    if s.is_empty() {
        return None;
    }
    if s == "*" {
        return Some(CachedChartDisplayBpm::Random);
    }
    let (min, max) = if let Some((a, b)) = s.split_once(':') {
        (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?)
    } else {
        let v = s.parse::<f64>().ok()?;
        (v, v)
    };
    if min.is_finite() && max.is_finite() && min > 0.0 && max > 0.0 {
        Some(CachedChartDisplayBpm::Specified {
            min: min.min(max),
            max: min.max(max),
        })
    } else {
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct SerializableChartData {
    chart_type: String,
    difficulty: String,
    description: String,
    chart_name: String,
    meter: u32,
    step_artist: String,
    notes: Vec<u8>,
    parsed_notes: Vec<CachedParsedNote>,
    row_to_beat: Vec<f32>,
    timing_segments: CachedTimingSegments,
    short_hash: String,
    stats: CachedArrowStats,
    tech_counts: CachedTechCounts,
    mines_nonfake: u32,
    stamina_counts: CachedStaminaCounts,
    total_streams: u32,
    matrix_rating: f64,
    max_nps: f64,
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
    chart_attacks: Option<String>,
    total_measures: usize,
    measure_nps_vec: Vec<f64>,
    display_bpm: Option<CachedChartDisplayBpm>,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
enum SerializableSongBackgroundChangeTarget {
    File(String),
    NoSongBg,
    Random,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct SerializableSongBackgroundChange {
    start_beat: f32,
    target: SerializableSongBackgroundChangeTarget,
}

impl From<&SongBackgroundChange> for SerializableSongBackgroundChange {
    fn from(change: &SongBackgroundChange) -> Self {
        let target = match &change.target {
            SongBackgroundChangeTarget::File(path) => {
                SerializableSongBackgroundChangeTarget::File(path.to_string_lossy().into_owned())
            }
            SongBackgroundChangeTarget::NoSongBg => {
                SerializableSongBackgroundChangeTarget::NoSongBg
            }
            SongBackgroundChangeTarget::Random => SerializableSongBackgroundChangeTarget::Random,
        };
        Self {
            start_beat: change.start_beat,
            target,
        }
    }
}

impl From<SerializableSongBackgroundChange> for SongBackgroundChange {
    fn from(change: SerializableSongBackgroundChange) -> Self {
        let target = match change.target {
            SerializableSongBackgroundChangeTarget::File(path) => {
                SongBackgroundChangeTarget::File(PathBuf::from(path))
            }
            SerializableSongBackgroundChangeTarget::NoSongBg => {
                SongBackgroundChangeTarget::NoSongBg
            }
            SerializableSongBackgroundChangeTarget::Random => SongBackgroundChangeTarget::Random,
        };
        Self {
            start_beat: change.start_beat,
            target,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct SerializableSongData {
    simfile_path: String,
    title: String,
    subtitle: String,
    translit_title: String,
    translit_subtitle: String,
    artist: String,
    banner_path: Option<String>,
    background_path: Option<String>,
    background_changes: Vec<SerializableSongBackgroundChange>,
    has_lua: bool,
    cdtitle_path: Option<String>,
    music_path: Option<String>,
    display_bpm: String,
    offset: f32,
    sample_start: Option<f32>,
    sample_length: Option<f32>,
    min_bpm: f64,
    max_bpm: f64,
    normalized_bpms: String,
    music_length_seconds: f32,
    total_length_seconds: i32,
    precise_last_second_seconds: f32,
    charts: Vec<SerializableChartData>,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedChartMeta {
    chart_type: String,
    difficulty: String,
    description: String,
    chart_name: String,
    meter: u32,
    step_artist: String,
    short_hash: String,
    stats: CachedArrowStats,
    tech_counts: CachedTechCounts,
    mines_nonfake: u32,
    stamina_counts: CachedStaminaCounts,
    total_streams: u32,
    matrix_rating: f64,
    max_nps: f64,
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
    total_measures: usize,
    measure_nps_vec: Vec<f64>,
    measure_seconds_vec: Vec<f32>,
    first_second: f32,
    has_note_data: bool,
    has_chart_attacks: bool,
    has_significant_timing_changes: bool,
    possible_grade_points: i32,
    holds_total: u32,
    rolls_total: u32,
    mines_total: u32,
    display_bpm: Option<CachedChartDisplayBpm>,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedSongMeta {
    simfile_path: String,
    title: String,
    subtitle: String,
    translit_title: String,
    translit_subtitle: String,
    artist: String,
    banner_path: Option<String>,
    background_path: Option<String>,
    background_changes: Vec<SerializableSongBackgroundChange>,
    has_lua: bool,
    cdtitle_path: Option<String>,
    music_path: Option<String>,
    display_bpm: String,
    offset: f32,
    sample_start: Option<f32>,
    sample_length: Option<f32>,
    min_bpm: f64,
    max_bpm: f64,
    normalized_bpms: String,
    music_length_seconds: f32,
    total_length_seconds: i32,
    precise_last_second_seconds: f32,
    charts: Vec<CachedChartMeta>,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct CachedChartPayload {
    notes: Vec<u8>,
    parsed_notes: Vec<CachedParsedNote>,
    row_to_beat: Vec<f32>,
    timing_segments: CachedTimingSegments,
    chart_attacks: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
struct CachedChartPayloadIndex {
    offset: u64,
    len: u64,
}

#[inline(always)]
fn chart_has_attacks(attacks: Option<&str>) -> bool {
    attacks.is_some_and(|attacks| !attacks.trim().is_empty())
}

fn chart_has_significant_timing_changes(timing: &TimingSegments) -> bool {
    if !timing.stops.is_empty()
        || !timing.delays.is_empty()
        || !timing.warps.is_empty()
        || !timing.speeds.is_empty()
        || !timing.scrolls.is_empty()
    {
        return true;
    }

    let mut min_bpm = f32::INFINITY;
    let mut max_bpm = 0.0_f32;
    for &(_, bpm) in &timing.bpms {
        if !bpm.is_finite() || bpm <= 0.0 {
            continue;
        }
        min_bpm = min_bpm.min(bpm);
        max_bpm = max_bpm.max(bpm);
    }

    min_bpm.is_finite() && max_bpm - min_bpm > 3.0
}

fn build_measure_seconds(timing: &TimingData, measure_count: usize) -> Vec<f32> {
    let mut seconds = Vec::with_capacity(measure_count);
    for measure in 0..measure_count {
        seconds.push(timing.get_time_for_beat((measure as f32) * 4.0));
    }
    seconds
}

fn build_chart_totals(
    parsed_notes: &[CachedParsedNote],
    timing: &TimingData,
) -> (i32, u32, u32, u32) {
    let mut holds_total = 0u32;
    let mut rolls_total = 0u32;
    let mut mines_total = 0u32;
    let mut rows: Vec<usize> = Vec::with_capacity(parsed_notes.len());
    for parsed in parsed_notes {
        let row_index = parsed.row_index as usize;
        let Some(beat) = timing.get_beat_for_row(row_index) else {
            continue;
        };
        let explicit_fake_tap = matches!(parsed.note_type, CachedNoteType::Fake);
        let fake_by_segment = timing.is_fake_at_beat(beat);
        let is_fake = explicit_fake_tap || fake_by_segment;
        let note_type: NoteType = parsed.note_type.into();
        let note_type = if explicit_fake_tap {
            NoteType::Tap
        } else {
            note_type
        };
        let can_be_judged = !is_fake && timing.is_judgable_at_beat(beat);
        if !can_be_judged {
            continue;
        }
        match note_type {
            NoteType::Hold => {
                holds_total = holds_total.saturating_add(1);
                rows.push(row_index);
            }
            NoteType::Roll => {
                rolls_total = rolls_total.saturating_add(1);
                rows.push(row_index);
            }
            NoteType::Mine => {
                mines_total = mines_total.saturating_add(1);
            }
            NoteType::Tap | NoteType::Lift | NoteType::Fake => {
                rows.push(row_index);
            }
        }
    }
    rows.sort_unstable();
    rows.dedup();
    let possible_i64 = i64::try_from(rows.len()).unwrap_or(i64::MAX) * 5
        + i64::from(holds_total) * i64::from(crate::game::judgment::HOLD_SCORE_HELD)
        + i64::from(rolls_total) * i64::from(crate::game::judgment::HOLD_SCORE_HELD);
    (
        possible_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        holds_total,
        rolls_total,
        mines_total,
    )
}

fn build_chart_meta(
    chart: SerializableChartData,
    song_offset: f32,
    global_offset_seconds: f32,
) -> ChartData {
    let timing_segments: TimingSegments = chart.timing_segments.into();
    let timing = TimingData::from_segments(
        -song_offset,
        global_offset_seconds,
        &timing_segments,
        &chart.row_to_beat,
    );
    let (possible_grade_points, holds_total, rolls_total, mines_total) =
        build_chart_totals(&chart.parsed_notes, &timing);
    let first_second = 0.0_f32.min(timing.get_time_for_beat(0.0));
    let measure_seconds_vec = build_measure_seconds(&timing, chart.measure_nps_vec.len());
    let has_chart_attacks = chart_has_attacks(chart.chart_attacks.as_deref());
    let has_significant_timing_changes = chart_has_significant_timing_changes(&timing_segments);
    ChartData {
        chart_type: chart.chart_type,
        difficulty: chart.difficulty,
        description: chart.description,
        chart_name: chart.chart_name,
        meter: chart.meter,
        step_artist: chart.step_artist,
        short_hash: chart.short_hash,
        stats: chart.stats.into(),
        tech_counts: chart.tech_counts.into(),
        mines_nonfake: chart.mines_nonfake,
        stamina_counts: chart.stamina_counts.into(),
        total_streams: chart.total_streams,
        matrix_rating: chart.matrix_rating,
        max_nps: chart.max_nps,
        sn_detailed_breakdown: chart.sn_detailed_breakdown,
        sn_partial_breakdown: chart.sn_partial_breakdown,
        sn_simple_breakdown: chart.sn_simple_breakdown,
        detailed_breakdown: chart.detailed_breakdown,
        partial_breakdown: chart.partial_breakdown,
        simple_breakdown: chart.simple_breakdown,
        total_measures: chart.total_measures,
        measure_nps_vec: chart.measure_nps_vec,
        measure_seconds_vec,
        first_second,
        has_note_data: !chart.notes.is_empty(),
        has_chart_attacks,
        has_significant_timing_changes,
        possible_grade_points,
        holds_total,
        rolls_total,
        mines_total,
        display_bpm: chart.display_bpm.map(Into::into),
    }
}

fn build_cached_chart_meta(
    chart: &SerializableChartData,
    song_offset: f32,
    global_offset_seconds: f32,
) -> CachedChartMeta {
    let timing_segments: TimingSegments = chart.timing_segments.clone().into();
    let timing = TimingData::from_segments(
        -song_offset,
        global_offset_seconds,
        &timing_segments,
        &chart.row_to_beat,
    );
    let (possible_grade_points, holds_total, rolls_total, mines_total) =
        build_chart_totals(&chart.parsed_notes, &timing);
    let first_second = 0.0_f32.min(timing.get_time_for_beat(0.0));
    let measure_seconds_vec = build_measure_seconds(&timing, chart.measure_nps_vec.len());
    CachedChartMeta {
        chart_type: chart.chart_type.clone(),
        difficulty: chart.difficulty.clone(),
        description: chart.description.clone(),
        chart_name: chart.chart_name.clone(),
        meter: chart.meter,
        step_artist: chart.step_artist.clone(),
        short_hash: chart.short_hash.clone(),
        stats: chart.stats.clone(),
        tech_counts: chart.tech_counts.clone(),
        mines_nonfake: chart.mines_nonfake,
        stamina_counts: chart.stamina_counts.clone(),
        total_streams: chart.total_streams,
        matrix_rating: chart.matrix_rating,
        max_nps: chart.max_nps,
        sn_detailed_breakdown: chart.sn_detailed_breakdown.clone(),
        sn_partial_breakdown: chart.sn_partial_breakdown.clone(),
        sn_simple_breakdown: chart.sn_simple_breakdown.clone(),
        detailed_breakdown: chart.detailed_breakdown.clone(),
        partial_breakdown: chart.partial_breakdown.clone(),
        simple_breakdown: chart.simple_breakdown.clone(),
        total_measures: chart.total_measures,
        measure_nps_vec: chart.measure_nps_vec.clone(),
        measure_seconds_vec,
        first_second,
        has_note_data: !chart.notes.is_empty(),
        has_chart_attacks: chart_has_attacks(chart.chart_attacks.as_deref()),
        has_significant_timing_changes: chart_has_significant_timing_changes(&timing_segments),
        possible_grade_points,
        holds_total,
        rolls_total,
        mines_total,
        display_bpm: chart.display_bpm.clone(),
    }
}

fn build_chart_meta_from_cache(chart: CachedChartMeta) -> ChartData {
    ChartData {
        chart_type: chart.chart_type,
        difficulty: chart.difficulty,
        description: chart.description,
        chart_name: chart.chart_name,
        meter: chart.meter,
        step_artist: chart.step_artist,
        short_hash: chart.short_hash,
        stats: chart.stats.into(),
        tech_counts: chart.tech_counts.into(),
        mines_nonfake: chart.mines_nonfake,
        stamina_counts: chart.stamina_counts.into(),
        total_streams: chart.total_streams,
        matrix_rating: chart.matrix_rating,
        max_nps: chart.max_nps,
        sn_detailed_breakdown: chart.sn_detailed_breakdown,
        sn_partial_breakdown: chart.sn_partial_breakdown,
        sn_simple_breakdown: chart.sn_simple_breakdown,
        detailed_breakdown: chart.detailed_breakdown,
        partial_breakdown: chart.partial_breakdown,
        simple_breakdown: chart.simple_breakdown,
        total_measures: chart.total_measures,
        measure_nps_vec: chart.measure_nps_vec,
        measure_seconds_vec: chart.measure_seconds_vec,
        first_second: chart.first_second,
        has_note_data: chart.has_note_data,
        has_chart_attacks: chart.has_chart_attacks,
        has_significant_timing_changes: chart.has_significant_timing_changes,
        possible_grade_points: chart.possible_grade_points,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
        display_bpm: chart.display_bpm.map(Into::into),
    }
}

fn build_gameplay_chart(
    chart: SerializableChartData,
    song_offset: f32,
    global_offset_seconds: f32,
) -> GameplayChartData {
    build_gameplay_chart_from_payload(
        CachedChartPayload {
            notes: chart.notes,
            parsed_notes: chart.parsed_notes,
            row_to_beat: chart.row_to_beat,
            timing_segments: chart.timing_segments,
            chart_attacks: chart.chart_attacks,
        },
        song_offset,
        global_offset_seconds,
    )
}

fn build_gameplay_chart_from_payload(
    chart: CachedChartPayload,
    song_offset: f32,
    global_offset_seconds: f32,
) -> GameplayChartData {
    let timing_segments: TimingSegments = chart.timing_segments.into();
    let row_to_beat = chart.row_to_beat;
    let timing = TimingData::from_segments(
        -song_offset,
        global_offset_seconds,
        &timing_segments,
        &row_to_beat,
    );
    GameplayChartData {
        notes: chart.notes,
        parsed_notes: chart
            .parsed_notes
            .into_iter()
            .map(ParsedNote::from)
            .collect(),
        row_to_beat,
        timing_segments,
        timing,
        chart_attacks: chart.chart_attacks,
    }
}

fn build_song_meta(song: SerializableSongData, global_offset_seconds: f32) -> SongData {
    let song_offset = song.offset;
    SongData {
        simfile_path: PathBuf::from(song.simfile_path),
        title: song.title,
        subtitle: song.subtitle,
        translit_title: song.translit_title,
        translit_subtitle: song.translit_subtitle,
        artist: song.artist,
        banner_path: song.banner_path.map(PathBuf::from),
        background_path: song.background_path.map(PathBuf::from),
        background_changes: song
            .background_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        has_lua: song.has_lua,
        cdtitle_path: song.cdtitle_path.map(PathBuf::from),
        music_path: song.music_path.map(PathBuf::from),
        display_bpm: song.display_bpm,
        offset: song.offset,
        sample_start: song.sample_start,
        sample_length: song.sample_length,
        min_bpm: song.min_bpm,
        max_bpm: song.max_bpm,
        normalized_bpms: song.normalized_bpms,
        music_length_seconds: song.music_length_seconds,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .into_iter()
            .map(|chart| build_chart_meta(chart, song_offset, global_offset_seconds))
            .collect(),
    }
}

fn build_cached_song_meta(
    song: &SerializableSongData,
    global_offset_seconds: f32,
) -> CachedSongMeta {
    let song_offset = song.offset;
    CachedSongMeta {
        simfile_path: song.simfile_path.clone(),
        title: song.title.clone(),
        subtitle: song.subtitle.clone(),
        translit_title: song.translit_title.clone(),
        translit_subtitle: song.translit_subtitle.clone(),
        artist: song.artist.clone(),
        banner_path: song.banner_path.clone(),
        background_path: song.background_path.clone(),
        background_changes: song.background_changes.clone(),
        has_lua: song.has_lua,
        cdtitle_path: song.cdtitle_path.clone(),
        music_path: song.music_path.clone(),
        display_bpm: song.display_bpm.clone(),
        offset: song.offset,
        sample_start: song.sample_start,
        sample_length: song.sample_length,
        min_bpm: song.min_bpm,
        max_bpm: song.max_bpm,
        normalized_bpms: song.normalized_bpms.clone(),
        music_length_seconds: song.music_length_seconds,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .iter()
            .map(|chart| build_cached_chart_meta(chart, song_offset, global_offset_seconds))
            .collect(),
    }
}

fn build_song_meta_from_cache(song: CachedSongMeta) -> SongData {
    SongData {
        simfile_path: PathBuf::from(song.simfile_path),
        title: song.title,
        subtitle: song.subtitle,
        translit_title: song.translit_title,
        translit_subtitle: song.translit_subtitle,
        artist: song.artist,
        banner_path: song.banner_path.map(PathBuf::from),
        background_path: song.background_path.map(PathBuf::from),
        background_changes: song
            .background_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        has_lua: song.has_lua,
        cdtitle_path: song.cdtitle_path.map(PathBuf::from),
        music_path: song.music_path.map(PathBuf::from),
        display_bpm: song.display_bpm,
        offset: song.offset,
        sample_start: song.sample_start,
        sample_length: song.sample_length,
        min_bpm: song.min_bpm,
        max_bpm: song.max_bpm,
        normalized_bpms: song.normalized_bpms,
        music_length_seconds: song.music_length_seconds,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .into_iter()
            .map(build_chart_meta_from_cache)
            .collect(),
    }
}

#[derive(Serialize, Deserialize, Encode, Decode)]
struct CachedSong {
    cache_version: u8,
    rssp_version: String,
    mono_threshold: usize,
    source_hash: u64,
    data: CachedSongMeta,
    chart_payloads: Vec<CachedChartPayloadIndex>,
}

// --- CACHING HELPER FUNCTIONS ---

pub(crate) fn fmt_scan_time(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{ms}ms");
    }
    if ms < 60_000 {
        return format!("{:.2}s", ms as f64 / 1000.0);
    }
    let total_s = ms as f64 / 1000.0;
    let m = (total_s / 60.0).floor() as u64;
    let s = (m as f64).mul_add(-60.0, total_s);
    format!("{m}m{s:.1}s")
}

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" { 8 } else { 4 }
}

fn update_precise_last_second(song: &mut SerializableSongData, global_offset_seconds: f32) {
    let has_non_edit = song
        .charts
        .iter()
        .any(|c| !c.difficulty.eq_ignore_ascii_case("edit"));
    let mut last = 0.0_f32;
    let song_offset = song.offset;

    for chart in &song.charts {
        if has_non_edit && chart.difficulty.eq_ignore_ascii_case("edit") {
            continue;
        }

        let mut last_row: Option<usize> = None;
        for note in &chart.parsed_notes {
            let row = note.tail_row_index.unwrap_or(note.row_index) as usize;
            last_row = Some(last_row.map_or(row, |prev| prev.max(row)));
        }

        let Some(row) = last_row else {
            continue;
        };
        let Some(beat) = chart.row_to_beat.get(row).copied() else {
            continue;
        };
        let timing_segments: TimingSegments = chart.timing_segments.clone().into();
        let timing = TimingData::from_segments(
            -song_offset,
            global_offset_seconds,
            &timing_segments,
            &chart.row_to_beat,
        );
        let sec = timing.get_time_for_beat(beat);
        if sec.is_finite() {
            last = last.max(sec.max(0.0));
        }
    }

    let fallback = song.total_length_seconds.max(0) as f32;
    song.precise_last_second_seconds = last.max(fallback);
}

/// Helper to load a song from cache OR parse it if needed.
/// Returns (`SongData`, `is_cache_hit`).
fn process_song(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_path = if fastload || cachesongs {
        cache::compute_song_cache_path(&simfile_path)
    } else {
        None
    };

    if fastload
        && let Some(cp) = cache_path.as_deref()
        && let Some(song_data) = cache::load_song_from_cache(&simfile_path, cp)
    {
        return Ok((song_data, true));
    }

    let song_data = parse_song_and_maybe_write_cache(
        &simfile_path,
        fastload,
        cachesongs,
        cache_path.as_deref(),
        global_offset_seconds,
    )?;
    Ok((song_data, false))
}

/// Re-parse one simfile and replace its in-memory song-cache entry.
///
/// This is used after writing sync edits to disk so immediate replays use the
/// updated timing without a full songs rescan.
pub fn reload_song_in_cache(simfile_path: &Path) -> Result<Arc<SongData>, String> {
    let config = crate::config::get();
    let global_offset_seconds = config.global_offset_seconds;
    let cachesongs = config.cachesongs;
    let (song_data, _) = process_song(
        simfile_path.to_path_buf(),
        false,
        cachesongs,
        global_offset_seconds,
    )?;
    let updated = Arc::new(song_data);

    let mut song_cache = get_song_cache();
    let mut replaced = false;
    for pack in song_cache.iter_mut() {
        for song in &mut pack.songs {
            if song.simfile_path == simfile_path {
                *song = updated.clone();
                replaced = true;
            }
        }
    }
    if !replaced {
        return Err(format!(
            "Song '{}' not found in song cache",
            simfile_path.display()
        ));
    }
    Ok(updated)
}

fn build_requested_gameplay_charts(
    song_data: &SerializableSongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let song_offset = song_data.offset;
    requested_chart_ixs
        .iter()
        .map(|&chart_ix| {
            let chart = song_data
                .charts
                .get(chart_ix)
                .cloned()
                .ok_or_else(|| format!("Chart index {chart_ix} out of range"))?;
            Ok(build_gameplay_chart(
                chart,
                song_offset,
                global_offset_seconds,
            ))
        })
        .collect()
}

fn load_gameplay_song_data(
    simfile_path: &Path,
    allow_cache_write: bool,
    global_offset_seconds: f32,
) -> Result<SerializableSongData, String> {
    let started = Instant::now();
    let cache_path = allow_cache_write
        .then(|| cache::compute_song_cache_path(simfile_path))
        .flatten();
    let need_hash = allow_cache_write && cache_path.is_some();
    let parse_started = Instant::now();
    let (mut song_data, content_hash) = parse_and_process_song_file(simfile_path, need_hash)?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;
    update_precise_last_second(&mut song_data, global_offset_seconds);
    let write_started = Instant::now();
    if allow_cache_write && let (Some(cp), Some(ch)) = (cache_path.as_deref(), content_hash) {
        cache::write_song_cache(cp, ch, &song_data, global_offset_seconds);
    }
    let write_ms = write_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 25.0 {
        info!(
            "Gameplay song data load: source=parse file={:?} parse_ms={parse_ms:.3} write_ms={write_ms:.3} elapsed_ms={total_ms:.3}",
            simfile_path.file_name().unwrap_or_default()
        );
    } else {
        debug!(
            "Gameplay song data load: source=parse file={:?} parse_ms={parse_ms:.3} write_ms={write_ms:.3} elapsed_ms={total_ms:.3}",
            simfile_path.file_name().unwrap_or_default()
        );
    }
    Ok(song_data)
}

pub fn load_gameplay_charts(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let started = Instant::now();
    let config = crate::config::get();
    let allow_cache_read = config.fastload || config.cachesongs;
    let allow_cache_write = config.cachesongs;
    let load_started = Instant::now();
    if allow_cache_read
        && let Some(charts) =
            cache::load_gameplay_charts_from_cache(song, requested_chart_ixs, global_offset_seconds)
    {
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        let total_ms = started.elapsed().as_secs_f64() * 1000.0;
        if total_ms >= 25.0 {
            info!(
                "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms=0.000 elapsed_ms={total_ms:.3}",
                song.title,
                requested_chart_ixs.len()
            );
        } else {
            debug!(
                "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms=0.000 elapsed_ms={total_ms:.3}",
                song.title,
                requested_chart_ixs.len()
            );
        }
        return Ok(charts);
    }

    let song_data =
        load_gameplay_song_data(&song.simfile_path, allow_cache_write, global_offset_seconds)?;
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    let build_started = Instant::now();
    let charts =
        build_requested_gameplay_charts(&song_data, requested_chart_ixs, global_offset_seconds)?;
    let build_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    let total_ms = started.elapsed().as_secs_f64() * 1000.0;
    if total_ms >= 25.0 {
        info!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms={build_ms:.3} elapsed_ms={total_ms:.3}",
            song.title,
            requested_chart_ixs.len()
        );
    } else {
        debug!(
            "Gameplay chart payload load: song='{}' requested={} load_ms={load_ms:.3} materialize_ms={build_ms:.3} elapsed_ms={total_ms:.3}",
            song.title,
            requested_chart_ixs.len()
        );
    }
    Ok(charts)
}

fn parse_song_and_maybe_write_cache(
    path: &Path,
    fastload: bool,
    cachesongs: bool,
    cache_path: Option<&Path>,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    if fastload {
        debug!("Cache miss for: {:?}", path.file_name().unwrap_or_default());
    } else {
        debug!(
            "Parsing (fastload disabled): {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    let need_hash = cachesongs && cache_path.is_some();
    let (mut song_data, content_hash) = parse_and_process_song_file(path, need_hash)?;
    update_precise_last_second(&mut song_data, global_offset_seconds);
    if cachesongs && let (Some(cp), Some(ch)) = (cache_path, content_hash) {
        cache::write_song_cache(cp, ch, &song_data, global_offset_seconds);
    }
    Ok(build_song_meta(song_data, global_offset_seconds))
}

#[inline]
fn build_stamina_counts(chart: &rssp::report::ChartSummary) -> StaminaCounts {
    let boxes = compute_box_counts(&chart.detected_patterns).total_boxes;
    let towers = count_pattern(&chart.detected_patterns, PatternVariant::TowerLR)
        + count_pattern(&chart.detected_patterns, PatternVariant::TowerUD)
        + count_pattern(&chart.detected_patterns, PatternVariant::TowerCornerLD)
        + count_pattern(&chart.detected_patterns, PatternVariant::TowerCornerLU)
        + count_pattern(&chart.detected_patterns, PatternVariant::TowerCornerRD)
        + count_pattern(&chart.detected_patterns, PatternVariant::TowerCornerRU);
    let triangles = count_pattern(&chart.detected_patterns, PatternVariant::TriangleLDL)
        + count_pattern(&chart.detected_patterns, PatternVariant::TriangleLUL)
        + count_pattern(&chart.detected_patterns, PatternVariant::TriangleRDR)
        + count_pattern(&chart.detected_patterns, PatternVariant::TriangleRUR);
    let doritos = count_pattern(&chart.detected_patterns, PatternVariant::DoritoLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::DoritoRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::DoritoInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::DoritoInvRight);
    let hip_breakers = count_pattern(&chart.detected_patterns, PatternVariant::HipBreakerLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::HipBreakerRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::HipBreakerInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::HipBreakerInvRight);
    let copters = count_pattern(&chart.detected_patterns, PatternVariant::CopterLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::CopterRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::CopterInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::CopterInvRight);
    let spirals = count_pattern(&chart.detected_patterns, PatternVariant::SpiralLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::SpiralRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::SpiralInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::SpiralInvRight);
    let staircases = count_pattern(&chart.detected_patterns, PatternVariant::StaircaseLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::StaircaseRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::StaircaseInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::StaircaseInvRight);
    let sweeps = count_pattern(&chart.detected_patterns, PatternVariant::SweepLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::SweepRight)
        + count_pattern(&chart.detected_patterns, PatternVariant::SweepInvLeft)
        + count_pattern(&chart.detected_patterns, PatternVariant::SweepInvRight);

    StaminaCounts {
        anchors: chart.anchor_left + chart.anchor_down + chart.anchor_up + chart.anchor_right,
        triangles,
        boxes,
        towers,
        doritos,
        hip_breakers,
        copters,
        spirals,
        candles: chart.candle_total,
        candle_percent: chart.candle_percent,
        staircases,
        mono: chart.mono_total,
        mono_percent: chart.mono_percent,
        sweeps,
    }
}

#[inline(always)]
fn collapse_song_asset_path(path: &str) -> String {
    let has_root = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::with_capacity(path.split('/').count());
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if parts.last().is_some_and(|last| *last != "..") {
                parts.pop();
            } else {
                parts.push("..");
            }
            continue;
        }
        parts.push(part);
    }
    let collapsed = parts.join("/");
    if has_root {
        if collapsed.is_empty() {
            "/".to_string()
        } else {
            format!("/{collapsed}")
        }
    } else {
        collapsed
    }
}

#[inline(always)]
fn resolve_song_dir_entry_ci(base: &Path, name: &str) -> Option<PathBuf> {
    let want = name.to_ascii_lowercase();
    let entries = fs::read_dir(base).ok()?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().to_ascii_lowercase() == want {
            return Some(entry.path());
        }
    }
    None
}

#[inline(always)]
fn resolve_song_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    let asset_tag = asset_tag.trim();
    if asset_tag.is_empty() {
        return None;
    }

    let collapsed = collapse_song_asset_path(&asset_tag.replace('\\', "/"));
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.starts_with('/') {
        let path = PathBuf::from(&collapsed);
        return path.exists().then_some(path);
    }

    let direct = song_dir.join(&collapsed);
    if direct.exists() {
        return Some(direct);
    }

    let mut path = song_dir.to_path_buf();
    let mut parts = collapsed
        .split('/')
        .filter(|part| !part.is_empty())
        .peekable();
    while let Some(part) = parts.next() {
        if part == "." {
            continue;
        }
        if part == ".." {
            if !path.pop() {
                return None;
            }
            continue;
        }
        let next = resolve_song_dir_entry_ci(&path, part).or_else(|| {
            let next = path.join(part);
            next.exists().then_some(next)
        })?;
        if parts.peek().is_some() && !next.is_dir() {
            return None;
        }
        path = next;
    }
    Some(path)
}

#[inline(always)]
fn resolve_song_asset_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    resolve_song_path_like_itg(song_dir, asset_tag).filter(|path| path.is_file())
}

#[inline(always)]
fn path_uses_lua_like_itg(path: &Path) -> bool {
    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lua"))
    {
        return true;
    }
    path.is_dir() && path.join("default.lua").is_file()
}

#[inline(always)]
fn starts_with_ci(slice: &[u8], tag: &[u8]) -> bool {
    slice
        .get(..tag.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(tag))
}

#[inline(always)]
fn find_byte(slice: &[u8], needle: u8) -> Option<usize> {
    let mut i = 0usize;
    while i < slice.len() {
        if slice[i] == needle {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn find_either_byte(slice: &[u8], a: u8, b: u8) -> Option<usize> {
    let mut i = 0usize;
    while i < slice.len() {
        if slice[i] == a || slice[i] == b {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn find_unescaped_semi_no_hash(slice: &[u8]) -> Option<usize> {
    let mut off = 0usize;
    let mut has_hash = false;
    while off < slice.len() {
        let rel = find_either_byte(&slice[off..], b';', b'#')?;
        let idx = off + rel;
        if slice[idx] == b'#' {
            has_hash = true;
            off = idx + 1;
            continue;
        }
        let mut bs = 0usize;
        let mut i = idx;
        while i > 0 && slice[i - 1] == b'\\' {
            bs += 1;
            i -= 1;
        }
        if bs & 1 == 0 {
            return (!has_hash).then_some(idx);
        }
        off = idx + 1;
    }
    None
}

#[inline(always)]
fn scan_tag_end(slice: &[u8], allow_nl: bool) -> Option<(usize, usize)> {
    if allow_nl && let Some(end) = find_unescaped_semi_no_hash(slice) {
        return Some((end, end + 1));
    }

    let mut i = 0usize;
    let mut bs_odd = false;
    while i < slice.len() {
        let b = slice[i];
        if b == b'\\' {
            bs_odd = !bs_odd;
            i += 1;
            continue;
        }
        let escaped = bs_odd;
        bs_odd = false;
        if b == b';' {
            if !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }
        if b == b':' {
            if !allow_nl && !escaped {
                return Some((i, i + 1));
            }
            i += 1;
            continue;
        }
        if matches!(b, b'\n' | b'\r') {
            let mut j = i + 1;
            if b == b'\r' && slice.get(j) == Some(&b'\n') {
                j += 1;
            }
            while j < slice.len()
                && slice[j].is_ascii_whitespace()
                && !matches!(slice[j], b'\n' | b'\r')
            {
                j += 1;
            }
            if slice.get(j) == Some(&b'#') {
                return Some((i, j));
            }
            if !allow_nl && slice.get(j) != Some(&b';') {
                return None;
            }
        }
        i += 1;
    }
    None
}

#[inline(always)]
fn parse_tag_val(data: &[u8], tag_len: usize, allow_nl: bool) -> Option<(&[u8], usize)> {
    let slice = data.get(tag_len..)?;
    let (end, next) = scan_tag_end(slice, allow_nl)?;
    Some((&slice[..end], tag_len + next))
}

fn extract_named_tag_values<'a>(data: &'a [u8], tags: &[&[u8]]) -> Vec<&'a [u8]> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let Some(pos) = find_byte(&data[i..], b'#') else {
            break;
        };
        i += pos;
        let slice = &data[i..];
        let Some(tag) = tags.iter().copied().find(|tag| starts_with_ci(slice, tag)) else {
            i += 1;
            continue;
        };
        if let Some((value, adv)) = parse_tag_val(slice, tag.len(), true) {
            out.push(value);
            i += adv;
        } else {
            i += 1;
        }
    }
    out
}

fn list_song_dir_rel_entries(song_dir: &Path) -> Vec<String> {
    let mut dirs = vec![song_dir.to_path_buf()];
    let mut entries = Vec::new();
    while let Some(dir) = dirs.pop() {
        let Ok(read_dir) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(song_dir) else {
                continue;
            };
            let rel = rel.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                dirs.push(path);
                entries.push(rel);
                continue;
            }
            if path.is_file() {
                entries.push(rel);
            }
        }
    }
    entries.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    entries
}

#[inline(always)]
fn strip_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        out.push_str(line);
    }
    out
}

fn match_bgchange_entry<'a>(changes: &'a str, start: usize, entries: &[String]) -> Option<&'a str> {
    for entry in entries {
        let Some(head) = changes.get(start..start + entry.len()) else {
            continue;
        };
        if !head.eq_ignore_ascii_case(entry) {
            continue;
        }
        let next = start + entry.len();
        if matches!(changes.as_bytes().get(next), None | Some(b'=') | Some(b',')) {
            return Some(head);
        }
    }
    None
}

fn split_bgchange_sets_like_itg(changes: &str, entries: &[String]) -> Vec<Vec<String>> {
    let changes = strip_newlines(changes);
    if changes.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut start = 0usize;
    let mut pnum = 0u8;
    while start <= changes.len() {
        if matches!(pnum, 1 | 7)
            && let Some(found) = match_bgchange_entry(&changes, start, entries)
        {
            out.last_mut().unwrap().push(found.to_string());
            start += found.len();
            if let Some(&delim) = changes.as_bytes().get(start) {
                pnum = if delim == b'=' { pnum + 1 } else { 0 };
                start += 1;
            }
            continue;
        }
        if pnum == 0 {
            out.push(Vec::new());
        }
        let rem = &changes[start..];
        let eq = rem.find('=').map(|i| start + i);
        let comma = rem.find(',').map(|i| start + i);
        let Some((end, next_pnum)) = eq
            .zip(comma)
            .map(|(e, c)| if e < c { (e, pnum + 1) } else { (c, 0) })
            .or_else(|| eq.map(|e| (e, pnum + 1)))
            .or_else(|| comma.map(|c| (c, 0)))
        else {
            out.last_mut().unwrap().push(changes[start..].to_string());
            break;
        };
        out.last_mut()
            .unwrap()
            .push(changes[start..end].to_string());
        start = end + 1;
        pnum = next_pnum;
    }
    out
}

#[inline(always)]
fn bgchange_target_uses_lua(song_dir: &Path, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty()
        || target.eq_ignore_ascii_case("-nosongbg-")
        || target.eq_ignore_ascii_case("-random-")
    {
        return false;
    }
    resolve_song_path_like_itg(song_dir, target).is_some_and(|path| path_uses_lua_like_itg(&path))
}

fn bgchange_values_use_lua(song_dir: &Path, values: &[&[u8]], entries: &[String]) -> bool {
    values.iter().copied().any(|raw| {
        let text = unescape_tag(decode_bytes(raw).as_ref()).into_owned();
        split_bgchange_sets_like_itg(&text, entries)
            .into_iter()
            .any(|fields| {
                fields
                    .get(1)
                    .is_some_and(|target| bgchange_target_uses_lua(song_dir, target))
            })
    })
}

fn simfile_uses_lua(song_dir: &Path, simfile_data: &[u8], background_tag: &str) -> bool {
    if resolve_song_path_like_itg(song_dir, background_tag)
        .is_some_and(|path| path_uses_lua_like_itg(&path))
    {
        return true;
    }
    let entries = list_song_dir_rel_entries(song_dir);
    bgchange_values_use_lua(song_dir, &extract_bgchanges_values(simfile_data), &entries)
        || bgchange_values_use_lua(
            song_dir,
            &extract_named_tag_values(simfile_data, &[b"#FGCHANGES:"]),
            &entries,
        )
}

fn convert_background_change(
    change: rssp::assets::ResolvedBackgroundChange,
) -> SongBackgroundChange {
    let target = match change.target {
        rssp::assets::BackgroundChangeTarget::File(path) => SongBackgroundChangeTarget::File(path),
        rssp::assets::BackgroundChangeTarget::NoSongBg => SongBackgroundChangeTarget::NoSongBg,
        rssp::assets::BackgroundChangeTarget::Random => SongBackgroundChangeTarget::Random,
    };
    SongBackgroundChange {
        start_beat: change.start_beat,
        target,
    }
}

/// The original parsing logic, now separated to be called on a cache miss.
fn parse_and_process_song_file(
    path: &Path,
    need_hash: bool,
) -> Result<(SerializableSongData, Option<u64>), String> {
    let simfile_data = fs::read(path).map_err(|e| format!("Could not read file: {e}"))?;
    let content_hash = need_hash.then(|| {
        let mut hasher = XxHash64::with_seed(0);
        hasher.write(&simfile_data);
        hasher.finish()
    });
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let options = AnalysisOptions {
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        ..AnalysisOptions::default()
    };

    let summary = analyze(&simfile_data, extension, &options)?;
    let charts: Vec<SerializableChartData> = summary
        .charts
        .into_iter()
        .map(|c| {
            let lanes = step_type_lanes(&c.step_type_str);
            let parsed_notes =
                crate::game::parsing::notes::parse_chart_notes(&c.minimized_note_data, lanes);
            let timing_segments = TimingSegments::from(c.timing_segments.as_ref());
            let stamina_counts = build_stamina_counts(&c);
            debug!(
                "  Chart '{}' [{}] loaded with {} bytes of note data.",
                c.difficulty_str,
                c.rating_str,
                c.minimized_note_data.len()
            );
            SerializableChartData {
                chart_type: c.step_type_str,
                difficulty: c.difficulty_str,
                description: c.description_str,
                chart_name: c.chart_name_str,
                meter: c.rating_str.parse().unwrap_or(0),
                step_artist: c.step_artist_str,
                notes: c.minimized_note_data,
                parsed_notes: parsed_notes.iter().map(CachedParsedNote::from).collect(),
                row_to_beat: c.row_to_beat,
                timing_segments: (&timing_segments).into(),
                short_hash: c.short_hash,
                stats: (&c.stats).into(),
                tech_counts: (&c.tech_counts).into(),
                mines_nonfake: c.mines_nonfake,
                stamina_counts: (&stamina_counts).into(),
                total_streams: c.total_streams,
                total_measures: c.total_measures,
                matrix_rating: c.matrix_rating,
                max_nps: c.max_nps,
                sn_detailed_breakdown: c.sn_detailed_breakdown,
                sn_partial_breakdown: c.sn_partial_breakdown,
                sn_simple_breakdown: c.sn_simple_breakdown,
                detailed_breakdown: c.detailed_breakdown,
                partial_breakdown: c.partial_breakdown,
                simple_breakdown: c.simple_breakdown,
                measure_nps_vec: c.measure_nps_vec,
                chart_attacks: c.chart_attacks,
                display_bpm: parse_chart_display_bpm(c.chart_display_bpm.as_deref()),
            }
        })
        .collect();

    let simfile_dir = path
        .parent()
        .ok_or_else(|| "Could not determine simfile directory".to_string())?;

    let (banner_path, background_path_opt) = rssp::assets::resolve_song_assets(
        simfile_dir,
        &summary.banner_path,
        &summary.background_path,
    );
    let has_lua = simfile_uses_lua(simfile_dir, &simfile_data, &summary.background_path);
    let background_changes =
        rssp::assets::resolve_background_changes_like_itg(simfile_dir, &simfile_data)
            .into_iter()
            .map(convert_background_change)
            .map(|change| SerializableSongBackgroundChange::from(&change))
            .collect();
    let cdtitle_path = resolve_song_asset_path_like_itg(simfile_dir, &summary.cdtitle_path);

    let music_path = resolve_song_asset_path_like_itg(simfile_dir, &summary.music_path)
        .or_else(|| rssp::assets::resolve_music_path_like_itg(simfile_dir, &summary.music_path));

    // Compute audio length (music file duration) in seconds, mirroring ITGmania's
    // m_fMusicLengthSeconds. This intentionally measures the full OGG length,
    // including trailing silence, and is used for displays that call
    // Song:MusicLengthSeconds() in Simply Love.
    //
    // StepMania also applies a safety heuristic: if the decoded music length
    // is suspiciously shorter than the chart's last second (by > 10s), it
    // trusts the chart timing instead. This handles meme files where the
    // audio is a short silent stub but the chart runs for hours.
    let mut music_length_seconds = compute_music_length_seconds(music_path.as_deref());
    let chart_length_seconds = summary.total_length.max(0) as f32;
    if music_length_seconds > 0.0
        && chart_length_seconds > 0.0
        && music_length_seconds < chart_length_seconds - 10.0
    {
        music_length_seconds = chart_length_seconds;
    }

    Ok((
        SerializableSongData {
            simfile_path: path.to_string_lossy().into_owned(),
            title: summary.title_str,
            subtitle: summary.subtitle_str,
            translit_title: summary.titletranslit_str,
            translit_subtitle: summary.subtitletranslit_str,
            artist: summary.artist_str,
            banner_path: banner_path.map(|p| p.to_string_lossy().into_owned()),
            background_path: background_path_opt.map(|p| p.to_string_lossy().into_owned()),
            background_changes,
            has_lua,
            cdtitle_path: cdtitle_path.map(|p| p.to_string_lossy().into_owned()),
            display_bpm: summary.display_bpm_str,
            offset: summary.offset as f32,
            sample_start: if summary.sample_start > 0.0 {
                Some(summary.sample_start as f32)
            } else {
                None
            },
            sample_length: if summary.sample_length > 0.0 {
                Some(summary.sample_length as f32)
            } else {
                None
            },
            min_bpm: summary.min_bpm,
            max_bpm: summary.max_bpm,
            normalized_bpms: summary.normalized_bpms,
            music_path: music_path.map(|p| p.to_string_lossy().into_owned()),
            music_length_seconds,
            total_length_seconds: summary.total_length,
            precise_last_second_seconds: summary.total_length.max(0) as f32,
            charts,
        },
        content_hash,
    ))
}

/// Computes the length of the music file in seconds when the decode layer supports it.
/// Returns 0.0 on failure or if no music path is provided.
fn compute_music_length_seconds(music_path: Option<&Path>) -> f32 {
    let Some(path) = music_path else {
        return 0.0;
    };
    match decode::file_length_seconds(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute audio length for {path:?}: {e}");
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::simfile_uses_lua;
    use std::fs;
    use std::path::PathBuf;

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-simfile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn simfile_uses_lua_detects_background_lua_file() {
        let root = test_dir("lua-background-file");
        let song_dir = root.join("Song");
        fs::create_dir_all(&song_dir).unwrap();
        fs::write(song_dir.join("modchart.lua"), "return Def.ActorFrame{}").unwrap();

        assert!(simfile_uses_lua(
            &song_dir,
            b"#TITLE:Lua Test;#BACKGROUND:modchart.lua;",
            "modchart.lua",
        ));
    }

    #[test]
    fn simfile_uses_lua_detects_fgchange_dir_default_lua() {
        let root = test_dir("lua-fgchange-dir");
        let song_dir = root.join("Song");
        let fg_dir = song_dir.join("Visuals");
        fs::create_dir_all(&fg_dir).unwrap();
        fs::write(fg_dir.join("default.lua"), "return Def.ActorFrame{}").unwrap();

        assert!(simfile_uses_lua(
            &song_dir,
            b"#TITLE:Lua Test;#FGCHANGES:0=Visuals=1=0=0=0=0;",
            "",
        ));
    }
}
