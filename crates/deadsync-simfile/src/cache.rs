use crate::song::SONG_ANALYSIS_MONO_THRESHOLD;
use bincode::{Decode, Encode};
use deadsync_chart::{
    ArrowStats, ChartData, ChartDisplayBpm, GameplayChartData, SongBackgroundChange,
    SongBackgroundChangeTarget, SongBackgroundLuaChange, SongData, SongForegroundChange,
    SongForegroundLuaChange, StaminaCounts, TechCounts, notes::ParsedNote,
};
use deadsync_core::note::NoteType;
use deadsync_rules::judgment::HOLD_SCORE_HELD;
use deadsync_rules::timing::{
    DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment,
    TimeSignatureSegment, TimingData, TimingSegments, WarpSegment, default_time_signatures,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, UNIX_EPOCH};
use twox_hash::XxHash64;

use crate::song::{ParseSongOptions, parse_song_data_file};

pub const SONG_CACHE_VERSION: u8 = 12;
pub const SONG_CACHE_MAGIC: [u8; 8] = *b"DSCACHE1";

#[derive(Debug)]
pub enum SongCacheWriteError {
    DirectoryHash(std::io::Error),
    EncodeHeader,
    EncodePayload,
    CreateDir {
        path: PathBuf,
        source: std::io::Error,
    },
    CreateFile(std::io::Error),
    Write(std::io::Error),
}

impl fmt::Display for SongCacheWriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectoryHash(error) => write!(f, "could not hash song directory: {error}"),
            Self::EncodeHeader => write!(f, "could not encode cache header"),
            Self::EncodePayload => write!(f, "could not encode chart payload"),
            Self::CreateDir { path, source } => {
                write!(f, "could not create cache directory {path:?}: {source}")
            }
            Self::CreateFile(error) => write!(f, "could not create cache file: {error}"),
            Self::Write(error) => write!(f, "could not write cache file: {error}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct CachedArrowStats {
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

impl From<CachedArrowStats> for ArrowStats {
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
pub struct CachedTechCounts {
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

impl From<CachedTechCounts> for TechCounts {
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
pub struct CachedStaminaCounts {
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

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub enum SerializableSongBackgroundChangeTarget {
    File(String),
    Animation(String),
    NoSongBg,
    Random,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableSongBackgroundChange {
    pub start_beat: f32,
    pub target: SerializableSongBackgroundChangeTarget,
    pub rate: f32,
    pub effect: String,
    pub file2: Option<String>,
    pub transition: String,
    pub color1: Option<[f32; 4]>,
    pub color2: Option<[f32; 4]>,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableSongForegroundLuaChange {
    pub start_beat: f32,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableSongForegroundChange {
    pub start_beat: f32,
    pub path: String,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableSongBackgroundLuaChange {
    pub start_beat: f32,
    pub path: String,
}

impl From<&SongBackgroundChange> for SerializableSongBackgroundChange {
    fn from(change: &SongBackgroundChange) -> Self {
        let target = match &change.target {
            SongBackgroundChangeTarget::File(path) => {
                SerializableSongBackgroundChangeTarget::File(path.to_string_lossy().into_owned())
            }
            SongBackgroundChangeTarget::Animation(name) => {
                SerializableSongBackgroundChangeTarget::Animation(name.clone())
            }
            SongBackgroundChangeTarget::NoSongBg => {
                SerializableSongBackgroundChangeTarget::NoSongBg
            }
            SongBackgroundChangeTarget::Random => SerializableSongBackgroundChangeTarget::Random,
        };
        Self {
            start_beat: change.start_beat,
            target,
            rate: change.rate,
            effect: change.effect.clone(),
            file2: change
                .file2
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            transition: change.transition.clone(),
            color1: change.color1,
            color2: change.color2,
        }
    }
}

impl From<SerializableSongBackgroundChange> for SongBackgroundChange {
    fn from(change: SerializableSongBackgroundChange) -> Self {
        let target = match change.target {
            SerializableSongBackgroundChangeTarget::File(path) => {
                SongBackgroundChangeTarget::File(PathBuf::from(path))
            }
            SerializableSongBackgroundChangeTarget::Animation(name) => {
                SongBackgroundChangeTarget::Animation(name)
            }
            SerializableSongBackgroundChangeTarget::NoSongBg => {
                SongBackgroundChangeTarget::NoSongBg
            }
            SerializableSongBackgroundChangeTarget::Random => SongBackgroundChangeTarget::Random,
        };
        Self {
            start_beat: change.start_beat,
            target,
            rate: change.rate,
            effect: change.effect,
            file2: change.file2.map(PathBuf::from),
            transition: change.transition,
            color1: change.color1,
            color2: change.color2,
        }
    }
}

impl From<&SongForegroundLuaChange> for SerializableSongForegroundLuaChange {
    fn from(change: &SongForegroundLuaChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: change.path.to_string_lossy().into_owned(),
        }
    }
}

impl From<SerializableSongForegroundLuaChange> for SongForegroundLuaChange {
    fn from(change: SerializableSongForegroundLuaChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: PathBuf::from(change.path),
        }
    }
}

impl From<&SongForegroundChange> for SerializableSongForegroundChange {
    fn from(change: &SongForegroundChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: change.path.to_string_lossy().into_owned(),
        }
    }
}

impl From<SerializableSongForegroundChange> for SongForegroundChange {
    fn from(change: SerializableSongForegroundChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: PathBuf::from(change.path),
        }
    }
}

impl From<&SongBackgroundLuaChange> for SerializableSongBackgroundLuaChange {
    fn from(change: &SongBackgroundLuaChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: change.path.to_string_lossy().into_owned(),
        }
    }
}

impl From<SerializableSongBackgroundLuaChange> for SongBackgroundLuaChange {
    fn from(change: SerializableSongBackgroundLuaChange) -> Self {
        Self {
            start_beat: change.start_beat,
            path: PathBuf::from(change.path),
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
pub struct CachedTimingSegments {
    beat0_offset_adjust: f32,
    bpms: Vec<(f32, f32)>,
    stops: Vec<(f32, f32)>,
    delays: Vec<(f32, f32)>,
    warps: Vec<(f32, f32)>,
    speeds: Vec<CachedSpeedSegment>,
    scrolls: Vec<(f32, f32)>,
    fakes: Vec<(f32, f32)>,
    time_signatures: Vec<(f32, i32, i32)>,
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
            time_signatures: segments
                .time_signatures
                .iter()
                .map(|seg| (seg.beat, seg.numerator, seg.denominator))
                .collect(),
        }
    }
}

impl From<CachedTimingSegments> for TimingSegments {
    fn from(segments: CachedTimingSegments) -> Self {
        let time_signatures: Vec<TimeSignatureSegment> = segments
            .time_signatures
            .into_iter()
            .map(|(beat, numerator, denominator)| TimeSignatureSegment {
                beat,
                numerator,
                denominator,
            })
            .collect();
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
            time_signatures: if time_signatures.is_empty() {
                default_time_signatures()
            } else {
                time_signatures
            },
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode, PartialEq, Eq)]
pub enum CachedNoteType {
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
pub struct CachedParsedNote {
    pub row_index: u32,
    pub column: u8,
    pub note_type: CachedNoteType,
    pub tail_row_index: Option<u32>,
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
pub struct CachedChartPayload {
    pub notes: Vec<u8>,
    pub parsed_notes: Vec<CachedParsedNote>,
    pub row_to_beat: Vec<f32>,
    pub timing_segments: CachedTimingSegments,
    pub chart_attacks: Option<String>,
}

#[derive(Encode)]
struct BorrowedCachedChartPayload<'a> {
    notes: &'a [u8],
    parsed_notes: &'a [CachedParsedNote],
    row_to_beat: &'a [f32],
    timing_segments: &'a CachedTimingSegments,
    chart_attacks: Option<&'a str>,
}

impl<'a> From<&'a SerializableChartData> for BorrowedCachedChartPayload<'a> {
    fn from(chart: &'a SerializableChartData) -> Self {
        Self {
            notes: &chart.notes,
            parsed_notes: &chart.parsed_notes,
            row_to_beat: &chart.row_to_beat,
            timing_segments: &chart.timing_segments,
            chart_attacks: chart.chart_attacks.as_deref(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Encode, Decode)]
pub struct CachedChartPayloadIndex {
    pub offset: u64,
    pub len: u64,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub enum CachedChartDisplayBpm {
    Specified { min: f64, max: f64 },
    Random,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableChartData {
    pub chart_type: String,
    pub difficulty: String,
    pub description: String,
    pub chart_name: String,
    pub meter: u32,
    pub step_artist: String,
    pub music_path: Option<String>,
    pub notes: Vec<u8>,
    pub parsed_notes: Vec<CachedParsedNote>,
    pub row_to_beat: Vec<f32>,
    pub timing_segments: CachedTimingSegments,
    pub short_hash: String,
    pub stats: CachedArrowStats,
    pub tech_counts: CachedTechCounts,
    pub mines_nonfake: u32,
    pub stamina_counts: CachedStaminaCounts,
    pub total_streams: u32,
    pub matrix_rating: f64,
    pub max_nps: f64,
    pub sn_detailed_breakdown: String,
    pub sn_partial_breakdown: String,
    pub sn_simple_breakdown: String,
    pub detailed_breakdown: String,
    pub partial_breakdown: String,
    pub simple_breakdown: String,
    pub chart_attacks: Option<String>,
    pub total_measures: usize,
    pub measure_nps_vec: Vec<f64>,
    pub display_bpm: Option<CachedChartDisplayBpm>,
    pub min_bpm: f64,
    pub max_bpm: f64,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct SerializableSongData {
    pub simfile_path: String,
    pub title: String,
    pub subtitle: String,
    pub translit_title: String,
    pub translit_subtitle: String,
    pub artist: String,
    pub genre: String,
    pub banner_path: Option<String>,
    pub background_path: Option<String>,
    pub background_changes: Vec<SerializableSongBackgroundChange>,
    pub background_layer2_changes: Vec<SerializableSongBackgroundChange>,
    pub foreground_changes: Vec<SerializableSongForegroundChange>,
    pub background_lua_changes: Vec<SerializableSongBackgroundLuaChange>,
    pub foreground_lua_changes: Vec<SerializableSongForegroundLuaChange>,
    pub has_lua: bool,
    pub cdtitle_path: Option<String>,
    pub music_path: Option<String>,
    pub display_bpm: String,
    pub offset: f32,
    pub sample_start: Option<f32>,
    pub sample_length: Option<f32>,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_bpms: String,
    pub music_length_seconds: f32,
    pub first_second: f32,
    pub total_length_seconds: i32,
    pub precise_last_second_seconds: f32,
    pub charts: Vec<SerializableChartData>,
}

pub fn update_precise_song_bounds(song: &mut SerializableSongData, global_offset_seconds: f32) {
    let has_non_edit = song
        .charts
        .iter()
        .any(|chart| !chart.difficulty.eq_ignore_ascii_case("edit"));
    let mut first = f32::INFINITY;
    let mut last = 0.0_f32;
    let song_offset = song.offset;

    for chart in &song.charts {
        if !song_length_chart_candidate(chart, has_non_edit) {
            continue;
        }

        let mut first_row: Option<usize> = None;
        let mut last_row: Option<usize> = None;
        for note in &chart.parsed_notes {
            let head_row = note.row_index as usize;
            let row = note.tail_row_index.unwrap_or(note.row_index) as usize;
            first_row = Some(first_row.map_or(head_row, |prev| prev.min(head_row)));
            last_row = Some(last_row.map_or(row, |prev| prev.max(row)));
        }

        let Some(row) = last_row else {
            continue;
        };
        if row == 0 {
            continue;
        }
        let Some(first_row) = first_row else {
            continue;
        };
        let Some(first_beat) = chart.row_to_beat.get(first_row).copied() else {
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
        let first_sec = timing.get_time_for_beat(first_beat);
        if first_sec.is_finite() {
            first = first.min(first_sec);
        }
        let sec = timing.get_time_for_beat(beat);
        if sec.is_finite() {
            last = last.max(sec.max(0.0));
        }
    }

    song.first_second = if first.is_finite() && first < last {
        first
    } else {
        0.0
    };
    let fallback = song.total_length_seconds.max(0) as f32;
    song.precise_last_second_seconds = last.max(fallback);
}

fn song_length_chart_candidate(chart: &SerializableChartData, has_non_edit: bool) -> bool {
    if has_non_edit && chart.difficulty.eq_ignore_ascii_case("edit") {
        return false;
    }
    !chart.chart_type.eq_ignore_ascii_case("lights-cabinet")
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct CachedChartMeta {
    pub chart_type: String,
    pub difficulty: String,
    pub description: String,
    pub chart_name: String,
    pub meter: u32,
    pub step_artist: String,
    pub music_path: Option<String>,
    pub short_hash: String,
    pub stats: CachedArrowStats,
    pub tech_counts: CachedTechCounts,
    pub mines_nonfake: u32,
    pub stamina_counts: CachedStaminaCounts,
    pub total_streams: u32,
    pub matrix_rating: f64,
    pub max_nps: f64,
    pub sn_detailed_breakdown: String,
    pub sn_partial_breakdown: String,
    pub sn_simple_breakdown: String,
    pub detailed_breakdown: String,
    pub partial_breakdown: String,
    pub simple_breakdown: String,
    pub total_measures: usize,
    pub measure_nps_vec: Vec<f64>,
    pub measure_seconds_vec: Vec<f32>,
    pub first_second: f32,
    pub has_note_data: bool,
    pub has_chart_attacks: bool,
    pub possible_grade_points: i32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub display_bpm: Option<CachedChartDisplayBpm>,
    pub min_bpm: f64,
    pub max_bpm: f64,
}

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
pub struct CachedSongMeta {
    pub simfile_path: String,
    pub title: String,
    pub subtitle: String,
    pub translit_title: String,
    pub translit_subtitle: String,
    pub artist: String,
    pub genre: String,
    pub banner_path: Option<String>,
    pub background_path: Option<String>,
    pub background_changes: Vec<SerializableSongBackgroundChange>,
    pub background_layer2_changes: Vec<SerializableSongBackgroundChange>,
    pub foreground_changes: Vec<SerializableSongForegroundChange>,
    pub background_lua_changes: Vec<SerializableSongBackgroundLuaChange>,
    pub foreground_lua_changes: Vec<SerializableSongForegroundLuaChange>,
    pub has_lua: bool,
    pub cdtitle_path: Option<String>,
    pub music_path: Option<String>,
    pub display_bpm: String,
    pub offset: f32,
    pub sample_start: Option<f32>,
    pub sample_length: Option<f32>,
    pub min_bpm: f64,
    pub max_bpm: f64,
    pub normalized_bpms: String,
    pub music_length_seconds: f32,
    pub first_second: f32,
    pub total_length_seconds: i32,
    pub precise_last_second_seconds: f32,
    pub charts: Vec<CachedChartMeta>,
}

#[derive(Serialize, Deserialize, Encode, Decode)]
pub struct CachedSong {
    pub cache_version: u8,
    pub rssp_version: String,
    pub mono_threshold: usize,
    pub directory_hash: u64,
    pub data: CachedSongMeta,
    pub chart_payloads: Vec<CachedChartPayloadIndex>,
}

impl From<CachedChartDisplayBpm> for ChartDisplayBpm {
    fn from(c: CachedChartDisplayBpm) -> Self {
        match c {
            CachedChartDisplayBpm::Specified { min, max } => Self::Specified { min, max },
            CachedChartDisplayBpm::Random => Self::Random,
        }
    }
}

impl From<&ChartDisplayBpm> for CachedChartDisplayBpm {
    fn from(c: &ChartDisplayBpm) -> Self {
        match c {
            ChartDisplayBpm::Specified { min, max } => Self::Specified {
                min: *min,
                max: *max,
            },
            ChartDisplayBpm::Random => Self::Random,
        }
    }
}

pub fn parse_chart_display_bpm(tag: Option<&str>) -> Option<CachedChartDisplayBpm> {
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

pub fn build_chart_totals(
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
        let explicit_fake_tap = parsed.note_type == CachedNoteType::Fake;
        let fake_by_segment = timing.is_fake_at_beat(beat);
        let is_fake = explicit_fake_tap || fake_by_segment;
        let note_type = if explicit_fake_tap {
            NoteType::Tap
        } else {
            parsed.note_type.into()
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
        + i64::from(holds_total) * i64::from(HOLD_SCORE_HELD)
        + i64::from(rolls_total) * i64::from(HOLD_SCORE_HELD);
    (
        possible_i64.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32,
        holds_total,
        rolls_total,
        mines_total,
    )
}

#[inline(always)]
pub fn chart_has_attacks(attacks: Option<&str>) -> bool {
    attacks.is_some_and(|attacks| !attacks.trim().is_empty())
}

pub fn build_measure_seconds(timing: &TimingData, measure_count: usize) -> Vec<f32> {
    let mut seconds = Vec::with_capacity(measure_count);
    for measure in 0..measure_count {
        seconds.push(timing.get_time_for_beat((measure as f32) * 4.0));
    }
    seconds
}

pub fn build_gameplay_chart_from_payload(
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

pub fn build_chart_meta(
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
    ChartData {
        chart_type: chart.chart_type,
        difficulty: chart.difficulty,
        description: chart.description,
        chart_name: chart.chart_name,
        meter: chart.meter,
        step_artist: chart.step_artist,
        music_path: chart.music_path.map(PathBuf::from),
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
        possible_grade_points,
        holds_total,
        rolls_total,
        mines_total,
        display_bpm: chart.display_bpm.map(Into::into),
        min_bpm: chart.min_bpm,
        max_bpm: chart.max_bpm,
    }
}

pub fn build_cached_chart_meta(
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
        music_path: chart.music_path.clone(),
        short_hash: chart.short_hash.clone(),
        stats: chart.stats.clone(),
        tech_counts: chart.tech_counts,
        mines_nonfake: chart.mines_nonfake,
        stamina_counts: chart.stamina_counts,
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
        possible_grade_points,
        holds_total,
        rolls_total,
        mines_total,
        display_bpm: chart.display_bpm.clone(),
        min_bpm: chart.min_bpm,
        max_bpm: chart.max_bpm,
    }
}

pub fn build_chart_meta_from_cache(chart: CachedChartMeta) -> ChartData {
    ChartData {
        chart_type: chart.chart_type,
        difficulty: chart.difficulty,
        description: chart.description,
        chart_name: chart.chart_name,
        meter: chart.meter,
        step_artist: chart.step_artist,
        music_path: chart.music_path.map(PathBuf::from),
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
        possible_grade_points: chart.possible_grade_points,
        holds_total: chart.holds_total,
        rolls_total: chart.rolls_total,
        mines_total: chart.mines_total,
        display_bpm: chart.display_bpm.map(Into::into),
        min_bpm: chart.min_bpm,
        max_bpm: chart.max_bpm,
    }
}

pub fn build_gameplay_chart(
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

fn build_gameplay_chart_from_ref(
    chart: &SerializableChartData,
    song_offset: f32,
    global_offset_seconds: f32,
) -> GameplayChartData {
    build_gameplay_chart_from_payload(
        CachedChartPayload {
            notes: chart.notes.clone(),
            parsed_notes: chart.parsed_notes.clone(),
            row_to_beat: chart.row_to_beat.clone(),
            timing_segments: chart.timing_segments.clone(),
            chart_attacks: chart.chart_attacks.clone(),
        },
        song_offset,
        global_offset_seconds,
    )
}

pub fn build_requested_gameplay_charts(
    song: &SerializableSongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let song_offset = song.offset;
    requested_chart_ixs
        .iter()
        .map(|&chart_ix| {
            let chart = song
                .charts
                .get(chart_ix)
                .ok_or_else(|| format!("Chart index {chart_ix} out of range"))?;
            Ok(build_gameplay_chart_from_ref(
                chart,
                song_offset,
                global_offset_seconds,
            ))
        })
        .collect()
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn build_requested_gameplay_charts_legacy(
    song: &SerializableSongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Result<Vec<GameplayChartData>, String> {
    let song_offset = song.offset;
    requested_chart_ixs
        .iter()
        .map(|&chart_ix| {
            let chart = song
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

pub fn build_song_meta(song: SerializableSongData, global_offset_seconds: f32) -> SongData {
    let song_offset = song.offset;
    SongData {
        simfile_path: PathBuf::from(song.simfile_path),
        title: song.title,
        subtitle: song.subtitle,
        translit_title: song.translit_title,
        translit_subtitle: song.translit_subtitle,
        artist: song.artist,
        genre: song.genre,
        banner_path: song.banner_path.map(PathBuf::from),
        background_path: song.background_path.map(PathBuf::from),
        background_changes: song
            .background_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        background_layer2_changes: song
            .background_layer2_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        foreground_changes: song
            .foreground_changes
            .into_iter()
            .map(SongForegroundChange::from)
            .collect(),
        background_lua_changes: song
            .background_lua_changes
            .into_iter()
            .map(SongBackgroundLuaChange::from)
            .collect(),
        foreground_lua_changes: song
            .foreground_lua_changes
            .into_iter()
            .map(SongForegroundLuaChange::from)
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
        first_second: song.first_second,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .into_iter()
            .map(|chart| build_chart_meta(chart, song_offset, global_offset_seconds))
            .collect(),
    }
}

pub fn build_cached_song_meta(
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
        genre: song.genre.clone(),
        banner_path: song.banner_path.clone(),
        background_path: song.background_path.clone(),
        background_changes: song.background_changes.clone(),
        background_layer2_changes: song.background_layer2_changes.clone(),
        foreground_changes: song.foreground_changes.clone(),
        background_lua_changes: song.background_lua_changes.clone(),
        foreground_lua_changes: song.foreground_lua_changes.clone(),
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
        first_second: song.first_second,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .iter()
            .map(|chart| build_cached_chart_meta(chart, song_offset, global_offset_seconds))
            .collect(),
    }
}

pub fn build_song_meta_from_cache(song: CachedSongMeta) -> SongData {
    SongData {
        simfile_path: PathBuf::from(song.simfile_path),
        title: song.title,
        subtitle: song.subtitle,
        translit_title: song.translit_title,
        translit_subtitle: song.translit_subtitle,
        artist: song.artist,
        genre: song.genre,
        banner_path: song.banner_path.map(PathBuf::from),
        background_path: song.background_path.map(PathBuf::from),
        background_changes: song
            .background_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        background_layer2_changes: song
            .background_layer2_changes
            .into_iter()
            .map(SongBackgroundChange::from)
            .collect(),
        foreground_changes: song
            .foreground_changes
            .into_iter()
            .map(SongForegroundChange::from)
            .collect(),
        background_lua_changes: song
            .background_lua_changes
            .into_iter()
            .map(SongBackgroundLuaChange::from)
            .collect(),
        foreground_lua_changes: song
            .foreground_lua_changes
            .into_iter()
            .map(SongForegroundLuaChange::from)
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
        first_second: song.first_second,
        total_length_seconds: song.total_length_seconds,
        precise_last_second_seconds: song.precise_last_second_seconds,
        charts: song
            .charts
            .into_iter()
            .map(build_chart_meta_from_cache)
            .collect(),
    }
}

pub fn song_cache_path(cache_dir: &Path, simfile_path: &Path) -> Result<PathBuf, std::io::Error> {
    let canonical_path = simfile_path.canonicalize()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical_path.to_string_lossy().as_bytes());
    let hash = hasher.finish();
    let hash_hex = format!("{hash:016x}");
    let shard2 = &hash_hex[..2];
    Ok(cache_dir.join(shard2).join(format!("{hash_hex}.bin")))
}

pub fn load_song_cache_file(
    path: &Path,
    cache_path: &Path,
    verify_freshness: bool,
) -> Option<SongData> {
    let cached_song = load_cached_song(path, cache_path, verify_freshness)?;
    Some(build_song_meta_from_cache(cached_song.data))
}

fn encode_chart_payloads_borrowed(
    data: &SerializableSongData,
) -> Result<Vec<Vec<u8>>, bincode::error::EncodeError> {
    data.charts
        .iter()
        .map(|chart| {
            bincode::encode_to_vec(
                BorrowedCachedChartPayload::from(chart),
                bincode::config::standard(),
            )
        })
        .collect()
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn encode_chart_payloads_for_bench(data: &SerializableSongData) -> Vec<Vec<u8>> {
    encode_chart_payloads_borrowed(data).expect("encode borrowed chart payloads")
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn encode_chart_payloads_legacy_for_bench(data: &SerializableSongData) -> Vec<Vec<u8>> {
    data.charts
        .iter()
        .map(|chart| CachedChartPayload {
            notes: chart.notes.clone(),
            parsed_notes: chart.parsed_notes.clone(),
            row_to_beat: chart.row_to_beat.clone(),
            timing_segments: chart.timing_segments.clone(),
            chart_attacks: chart.chart_attacks.clone(),
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|payload| {
            bincode::encode_to_vec(payload, bincode::config::standard())
                .expect("encode owned chart payload")
        })
        .collect()
}

pub fn write_song_cache_file(
    cache_path: &Path,
    data: &SerializableSongData,
    global_offset_seconds: f32,
) -> Result<(), SongCacheWriteError> {
    let directory_hash = get_song_directory_hash(Path::new(&data.simfile_path))
        .map_err(SongCacheWriteError::DirectoryHash)?;
    let encoded_payloads =
        encode_chart_payloads_borrowed(data).map_err(|_| SongCacheWriteError::EncodePayload)?;
    let meta = build_cached_song_meta(data, global_offset_seconds);
    let mut chart_payloads = Vec::with_capacity(encoded_payloads.len());
    let mut payload_offset = 0u64;
    for encoded in &encoded_payloads {
        let len = encoded.len() as u64;
        chart_payloads.push(CachedChartPayloadIndex {
            offset: payload_offset,
            len,
        });
        payload_offset = payload_offset.saturating_add(len);
    }
    let cached_song = CachedSong {
        cache_version: SONG_CACHE_VERSION,
        rssp_version: rssp::RSSP_VERSION.to_string(),
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        directory_hash,
        data: meta,
        chart_payloads,
    };
    let encoded_header = bincode::encode_to_vec(&cached_song, bincode::config::standard())
        .map_err(|_| SongCacheWriteError::EncodeHeader)?;
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|source| SongCacheWriteError::CreateDir {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let mut file = fs::File::create(cache_path).map_err(SongCacheWriteError::CreateFile)?;
    file.write_all(&SONG_CACHE_MAGIC)
        .map_err(SongCacheWriteError::Write)?;
    file.write_all(&(encoded_header.len() as u64).to_le_bytes())
        .map_err(SongCacheWriteError::Write)?;
    file.write_all(&encoded_header)
        .map_err(SongCacheWriteError::Write)?;
    for payload in encoded_payloads {
        file.write_all(&payload)
            .map_err(SongCacheWriteError::Write)?;
    }
    Ok(())
}

pub fn load_gameplay_charts_cache_file(
    song: &SongData,
    cache_path: &Path,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
    verify_freshness: bool,
) -> Option<Vec<GameplayChartData>> {
    let (cached_song, payload_start) =
        load_cached_song_for_gameplay(&song.simfile_path, cache_path, verify_freshness)?;
    let song_offset = cached_song.data.offset;
    collect_requested_cached_charts(requested_chart_ixs, |chart_ix| {
        let entry = *cached_song.chart_payloads.get(chart_ix)?;
        let payload = load_cached_chart_payload(cache_path, payload_start, entry)?;
        Some(build_gameplay_chart_from_payload(
            payload,
            song_offset,
            global_offset_seconds,
        ))
    })
}

fn collect_requested_cached_charts(
    requested_chart_ixs: &[usize],
    mut load_chart: impl FnMut(usize) -> Option<GameplayChartData>,
) -> Option<Vec<GameplayChartData>> {
    let mut charts = Vec::<GameplayChartData>::with_capacity(requested_chart_ixs.len());
    let mut loaded_positions = HashMap::<usize, usize>::with_capacity(requested_chart_ixs.len());
    for &chart_ix in requested_chart_ixs {
        if let Some(&loaded_position) = loaded_positions.get(&chart_ix) {
            charts.push(charts.get(loaded_position)?.clone());
            continue;
        }
        let chart = load_chart(chart_ix)?;
        loaded_positions.insert(chart_ix, charts.len());
        charts.push(chart);
    }
    Some(charts)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn collect_requested_cached_charts_for_bench(
    source: &[GameplayChartData],
    requested_chart_ixs: &[usize],
) -> Option<Vec<GameplayChartData>> {
    collect_requested_cached_charts(requested_chart_ixs, |chart_ix| {
        source.get(chart_ix).cloned()
    })
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn collect_requested_cached_charts_legacy_for_bench(
    source: &[GameplayChartData],
    requested_chart_ixs: &[usize],
) -> Option<Vec<GameplayChartData>> {
    let mut charts = Vec::with_capacity(requested_chart_ixs.len());
    let mut loaded = HashMap::<usize, GameplayChartData>::with_capacity(requested_chart_ixs.len());
    for &chart_ix in requested_chart_ixs {
        if let Some(chart) = loaded.get(&chart_ix) {
            charts.push(chart.clone());
            continue;
        }
        let chart = source.get(chart_ix)?.clone();
        loaded.insert(chart_ix, chart.clone());
        charts.push(chart);
    }
    Some(charts)
}

pub struct GameplayChartLoadOptions<'a> {
    pub cache_dir: &'a Path,
    pub parse_options: &'a ParseSongOptions,
    pub allow_cache_read: bool,
    pub allow_cache_write: bool,
    pub verify_cache_freshness: bool,
    pub global_offset_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayChartLoadSource {
    Cache { verify_freshness: bool },
    Parse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GameplayChartLoadWarning {
    CachePath { path: PathBuf, error: String },
    CacheWrite { simfile_name: String, error: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameplaySongDataLoadReport {
    pub parse_ms: f64,
    pub write_ms: f64,
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameplayChartLoadReport {
    pub source: GameplayChartLoadSource,
    pub requested_count: usize,
    pub load_ms: f64,
    pub materialize_ms: f64,
    pub elapsed_ms: f64,
    pub song_data_load: Option<GameplaySongDataLoadReport>,
    pub warnings: Vec<GameplayChartLoadWarning>,
}

pub struct GameplayChartLoadResult {
    pub charts: Vec<GameplayChartData>,
    pub report: GameplayChartLoadReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameplayChartLoadLogLevel {
    Debug,
    Info,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameplayChartLoadLogEntry {
    pub level: GameplayChartLoadLogLevel,
    pub message: String,
}

impl GameplayChartLoadLogEntry {
    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            level: GameplayChartLoadLogLevel::Debug,
            message: message.into(),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: GameplayChartLoadLogLevel::Info,
            message: message.into(),
        }
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: GameplayChartLoadLogLevel::Warn,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeSongLoadLogLevel {
    Debug,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSongLoadLogEntry {
    pub level: RuntimeSongLoadLogLevel,
    pub message: String,
}

impl RuntimeSongLoadLogEntry {
    pub fn debug(message: impl Into<String>) -> Self {
        Self {
            level: RuntimeSongLoadLogLevel::Debug,
            message: message.into(),
        }
    }

    pub fn warn(message: impl Into<String>) -> Self {
        Self {
            level: RuntimeSongLoadLogLevel::Warn,
            message: message.into(),
        }
    }
}

pub struct RuntimeSongLoadOptions<'a> {
    pub cache_dir: &'a Path,
    pub parse_options: &'a ParseSongOptions,
    pub fastload: bool,
    pub cachesongs: bool,
    pub verify_cache_freshness: bool,
    pub global_offset_seconds: f32,
}

pub struct RuntimeSongLoadResult {
    pub song: SongData,
    pub cache_hit: bool,
    pub log_entries: Vec<RuntimeSongLoadLogEntry>,
}

pub fn load_song_with_cache_options<F>(
    simfile_path: &Path,
    options: &RuntimeSongLoadOptions<'_>,
    music_len: F,
) -> Result<RuntimeSongLoadResult, String>
where
    F: FnOnce(Option<&Path>) -> f32,
{
    let mut log_entries = Vec::with_capacity(3);
    let cache_path = if options.fastload || options.cachesongs {
        runtime_song_cache_path(options.cache_dir, simfile_path, &mut log_entries)
    } else {
        None
    };

    if (options.fastload || options.cachesongs)
        && let Some(cache_path) = cache_path.as_deref()
        && let Some(song) =
            load_song_cache_file(simfile_path, cache_path, options.verify_cache_freshness)
    {
        log_entries.push(RuntimeSongLoadLogEntry::debug(format!(
            "Cache hit for: {:?}",
            simfile_path.file_name().unwrap_or_default()
        )));
        return Ok(RuntimeSongLoadResult {
            song,
            cache_hit: true,
            log_entries,
        });
    }

    log_entries.push(runtime_song_parse_log_entry(simfile_path, options.fastload));
    let song_data = parse_song_data_file(
        simfile_path,
        options.parse_options,
        options.global_offset_seconds,
        music_len,
    )?;
    if options.cachesongs
        && let Some(cache_path) = cache_path.as_deref()
        && let Err(error) =
            write_song_cache_file(cache_path, &song_data, options.global_offset_seconds)
    {
        log_entries.push(RuntimeSongLoadLogEntry::warn(format!(
            "Could not write song cache for {:?}: {error}",
            Path::new(&song_data.simfile_path)
                .file_name()
                .unwrap_or_default()
        )));
    }

    Ok(RuntimeSongLoadResult {
        song: build_song_meta(song_data, options.global_offset_seconds),
        cache_hit: false,
        log_entries,
    })
}

fn runtime_song_cache_path(
    cache_dir: &Path,
    simfile_path: &Path,
    log_entries: &mut Vec<RuntimeSongLoadLogEntry>,
) -> Option<PathBuf> {
    match song_cache_path(cache_dir, simfile_path) {
        Ok(path) => Some(path),
        Err(error) => {
            log_entries.push(RuntimeSongLoadLogEntry::warn(format!(
                "Could not generate cache path for {simfile_path:?}: {error}. Caching disabled for this file."
            )));
            None
        }
    }
}

fn runtime_song_parse_log_entry(simfile_path: &Path, fastload: bool) -> RuntimeSongLoadLogEntry {
    let file_name = simfile_path.file_name().unwrap_or_default();
    if fastload {
        RuntimeSongLoadLogEntry::debug(format!("Cache miss for: {file_name:?}"))
    } else {
        RuntimeSongLoadLogEntry::debug(format!("Parsing (fastload disabled): {file_name:?}"))
    }
}

pub fn gameplay_chart_load_log_entries(
    song: &SongData,
    report: &GameplayChartLoadReport,
) -> Vec<GameplayChartLoadLogEntry> {
    let mut entries = Vec::with_capacity(report.warnings.len() + 3);
    entries.extend(
        report
            .warnings
            .iter()
            .map(gameplay_chart_load_warning_log_entry),
    );
    if let GameplayChartLoadSource::Cache { verify_freshness } = report.source {
        entries.push(gameplay_chart_cache_hit_log_entry(song, verify_freshness));
    }
    if let Some(song_data_load) = &report.song_data_load {
        entries.push(gameplay_song_data_load_log_entry(song, song_data_load));
    }
    entries.push(gameplay_chart_payload_load_log_entry(song, report));
    entries
}

fn gameplay_chart_load_warning_log_entry(
    warning: &GameplayChartLoadWarning,
) -> GameplayChartLoadLogEntry {
    match warning {
        GameplayChartLoadWarning::CachePath { path, error } => {
            GameplayChartLoadLogEntry::warn(format!(
                "Could not generate cache path for {path:?}: {error}. Caching disabled for this file."
            ))
        }
        GameplayChartLoadWarning::CacheWrite {
            simfile_name,
            error,
        } => GameplayChartLoadLogEntry::warn(format!(
            "Could not write song cache for {simfile_name:?}: {error}"
        )),
    }
}

fn gameplay_chart_cache_hit_log_entry(
    song: &SongData,
    verify_freshness: bool,
) -> GameplayChartLoadLogEntry {
    let file_name = song.simfile_path.file_name().unwrap_or_default();
    if verify_freshness {
        GameplayChartLoadLogEntry::debug(format!("Gameplay cache hit for: {file_name:?}"))
    } else {
        GameplayChartLoadLogEntry::debug(format!(
            "Gameplay cache hit (no freshness check) for: {file_name:?}"
        ))
    }
}

fn gameplay_song_data_load_log_entry(
    song: &SongData,
    report: &GameplaySongDataLoadReport,
) -> GameplayChartLoadLogEntry {
    let file_name = song.simfile_path.file_name().unwrap_or_default();
    let message = format!(
        "Gameplay song data load: source=parse file={:?} parse_ms={:.3} write_ms={:.3} elapsed_ms={:.3}",
        file_name, report.parse_ms, report.write_ms, report.elapsed_ms
    );
    if report.elapsed_ms >= 25.0 {
        GameplayChartLoadLogEntry::info(message)
    } else {
        GameplayChartLoadLogEntry::debug(message)
    }
}

fn gameplay_chart_payload_load_log_entry(
    song: &SongData,
    report: &GameplayChartLoadReport,
) -> GameplayChartLoadLogEntry {
    let message = format!(
        "Gameplay chart payload load: song='{}' requested={} load_ms={:.3} materialize_ms={:.3} elapsed_ms={:.3}",
        song.title,
        report.requested_count,
        report.load_ms,
        report.materialize_ms,
        report.elapsed_ms
    );
    if report.elapsed_ms >= 25.0 {
        GameplayChartLoadLogEntry::info(message)
    } else {
        GameplayChartLoadLogEntry::debug(message)
    }
}

pub fn load_gameplay_charts_with_options<F>(
    song: &SongData,
    requested_chart_ixs: &[usize],
    options: &GameplayChartLoadOptions<'_>,
    music_len: F,
) -> Result<GameplayChartLoadResult, String>
where
    F: FnOnce(Option<&Path>) -> f32,
{
    let started = Instant::now();
    let mut warnings = Vec::new();
    let load_started = Instant::now();
    if options.allow_cache_read
        && let Some(charts) =
            load_gameplay_charts_from_cache_dir(song, requested_chart_ixs, options, &mut warnings)
    {
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        return Ok(GameplayChartLoadResult {
            charts,
            report: GameplayChartLoadReport {
                source: GameplayChartLoadSource::Cache {
                    verify_freshness: options.verify_cache_freshness,
                },
                requested_count: requested_chart_ixs.len(),
                load_ms,
                materialize_ms: 0.0,
                elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
                song_data_load: None,
                warnings,
            },
        });
    }

    let (song_data, song_data_load) = load_gameplay_song_data_with_options(
        &song.simfile_path,
        options,
        &mut warnings,
        music_len,
    )?;
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    let build_started = Instant::now();
    let charts = build_requested_gameplay_charts(
        &song_data,
        requested_chart_ixs,
        options.global_offset_seconds,
    )?;
    let materialize_ms = build_started.elapsed().as_secs_f64() * 1000.0;
    Ok(GameplayChartLoadResult {
        charts,
        report: GameplayChartLoadReport {
            source: GameplayChartLoadSource::Parse,
            requested_count: requested_chart_ixs.len(),
            load_ms,
            materialize_ms,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
            song_data_load: Some(song_data_load),
            warnings,
        },
    })
}

pub fn load_sync_analysis_chart_with_options<F>(
    song: &SongData,
    chart_ix: usize,
    options: &GameplayChartLoadOptions<'_>,
    music_len: F,
) -> Result<GameplayChartLoadResult, String>
where
    F: FnOnce(Option<&Path>) -> f32,
{
    let mut result = load_gameplay_charts_with_options(song, &[chart_ix], options, music_len)?;
    if result.charts.len() == 1 {
        return Ok(result);
    }
    result.charts.clear();
    Err(format!("Chart index {chart_ix} out of range"))
}

pub fn gameplay_song_cache_path(
    cache_dir: &Path,
    simfile_path: &Path,
    warnings: &mut Vec<GameplayChartLoadWarning>,
) -> Option<PathBuf> {
    match song_cache_path(cache_dir, simfile_path) {
        Ok(path) => Some(path),
        Err(error) => {
            warnings.push(GameplayChartLoadWarning::CachePath {
                path: simfile_path.to_path_buf(),
                error: error.to_string(),
            });
            None
        }
    }
}

fn load_gameplay_charts_from_cache_dir(
    song: &SongData,
    requested_chart_ixs: &[usize],
    options: &GameplayChartLoadOptions<'_>,
    warnings: &mut Vec<GameplayChartLoadWarning>,
) -> Option<Vec<GameplayChartData>> {
    let cache_path = gameplay_song_cache_path(options.cache_dir, &song.simfile_path, warnings)?;
    load_gameplay_charts_cache_file(
        song,
        &cache_path,
        requested_chart_ixs,
        options.global_offset_seconds,
        options.verify_cache_freshness,
    )
}

fn load_gameplay_song_data_with_options<F>(
    simfile_path: &Path,
    options: &GameplayChartLoadOptions<'_>,
    warnings: &mut Vec<GameplayChartLoadWarning>,
    music_len: F,
) -> Result<(SerializableSongData, GameplaySongDataLoadReport), String>
where
    F: FnOnce(Option<&Path>) -> f32,
{
    let started = Instant::now();
    let parse_started = Instant::now();
    let song_data = parse_song_data_file(
        simfile_path,
        options.parse_options,
        options.global_offset_seconds,
        music_len,
    )?;
    let parse_ms = parse_started.elapsed().as_secs_f64() * 1000.0;
    let write_started = Instant::now();
    if options.allow_cache_write
        && let Some(cache_path) =
            gameplay_song_cache_path(options.cache_dir, simfile_path, warnings)
        && let Err(error) =
            write_song_cache_file(&cache_path, &song_data, options.global_offset_seconds)
    {
        warnings.push(GameplayChartLoadWarning::CacheWrite {
            simfile_name: Path::new(&song_data.simfile_path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            error: error.to_string(),
        });
    }
    let write_ms = write_started.elapsed().as_secs_f64() * 1000.0;
    Ok((
        song_data,
        GameplaySongDataLoadReport {
            parse_ms,
            write_ms,
            elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        },
    ))
}

fn path_hash(path: &Path) -> u64 {
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(path.to_string_lossy().as_bytes());
    hasher.finish()
}

fn file_metadata_hash(path: &Path) -> Result<u64, std::io::Error> {
    let meta = fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_secs());
    Ok(modified.wrapping_add(meta.len()))
}

fn get_song_directory_hash(simfile_path: &Path) -> Result<u64, std::io::Error> {
    let parent = simfile_path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "simfile path has no parent directory",
        )
    })?;
    let dir = parent.canonicalize()?;
    let mut hash = path_hash(&dir);
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("._"))
        {
            continue;
        }
        hash = hash.wrapping_add(file_metadata_hash(&entry.path())?);
    }
    Ok(hash)
}

fn cached_path_exists(path_opt: Option<&str>) -> bool {
    match path_opt.map(str::trim) {
        None => true,
        Some("") => false,
        Some(path) => Path::new(path).is_file(),
    }
}

fn cached_song_paths_exist(song: &CachedSong) -> bool {
    let data = &song.data;
    let bgchange_paths_ok = data
        .background_changes
        .iter()
        .chain(data.background_layer2_changes.iter())
        .all(|change| {
            let target_ok = match &change.target {
                SerializableSongBackgroundChangeTarget::File(path) => {
                    cached_path_exists(Some(path))
                }
                SerializableSongBackgroundChangeTarget::Animation(_) => true,
                SerializableSongBackgroundChangeTarget::NoSongBg
                | SerializableSongBackgroundChangeTarget::Random => true,
            };
            target_ok && cached_path_exists(change.file2.as_deref())
        });
    let foreground_paths_ok = data
        .foreground_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let foreground_lua_paths_ok = data
        .foreground_lua_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let background_lua_paths_ok = data
        .background_lua_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    let chart_music_paths_ok = data
        .charts
        .iter()
        .all(|chart| cached_path_exists(chart.music_path.as_deref()));
    cached_path_exists(data.banner_path.as_deref())
        && cached_path_exists(data.background_path.as_deref())
        && bgchange_paths_ok
        && foreground_paths_ok
        && background_lua_paths_ok
        && foreground_lua_paths_ok
        && chart_music_paths_ok
        && cached_path_exists(data.cdtitle_path.as_deref())
        && cached_path_exists(data.music_path.as_deref())
}

fn load_cached_song_base(cache_path: &Path) -> Option<(CachedSong, u64)> {
    if !cache_path.exists() {
        return None;
    }
    let Ok(mut file) = fs::File::open(cache_path) else {
        return None;
    };
    let mut prefix = [0u8; 16];
    if file.read_exact(&mut prefix).is_err() {
        return None;
    }
    if prefix[..8] != SONG_CACHE_MAGIC {
        return None;
    }
    let header_len = u64::from_le_bytes(prefix[8..16].try_into().ok()?);
    let header_len_usize = usize::try_from(header_len).ok()?;
    let mut buffer = vec![0u8; header_len_usize];
    if file.read_exact(&mut buffer).is_err() {
        return None;
    }
    let Ok((cached_song, _)) =
        bincode::decode_from_slice::<CachedSong, _>(&buffer, bincode::config::standard())
    else {
        return None;
    };

    if cached_song.cache_version != SONG_CACHE_VERSION
        || cached_song.rssp_version != rssp::RSSP_VERSION
        || cached_song.mono_threshold != SONG_ANALYSIS_MONO_THRESHOLD
        || !cached_song_paths_exist(&cached_song)
    {
        return None;
    }
    Some((cached_song, 16 + header_len))
}

fn load_cached_song(path: &Path, cache_path: &Path, verify_freshness: bool) -> Option<CachedSong> {
    let (cached_song, _) = load_cached_song_base(cache_path)?;
    if verify_freshness {
        validate_directory_hash(path, &cached_song)?;
    }
    Some(cached_song)
}

fn load_cached_song_for_gameplay(
    path: &Path,
    cache_path: &Path,
    verify_freshness: bool,
) -> Option<(CachedSong, u64)> {
    let (cached_song, payload_start) = load_cached_song_base(cache_path)?;
    if verify_freshness {
        validate_directory_hash(path, &cached_song)?;
    }
    Some((cached_song, payload_start))
}

fn validate_directory_hash(path: &Path, cached_song: &CachedSong) -> Option<()> {
    (cached_song.directory_hash == get_song_directory_hash(path).ok()?).then_some(())
}

fn load_cached_chart_payload(
    cache_path: &Path,
    payload_start: u64,
    entry: CachedChartPayloadIndex,
) -> Option<CachedChartPayload> {
    let Ok(mut file) = fs::File::open(cache_path) else {
        return None;
    };
    if file
        .seek(SeekFrom::Start(payload_start.saturating_add(entry.offset)))
        .is_err()
    {
        return None;
    }
    let len = usize::try_from(entry.len).ok()?;
    let mut buffer = vec![0u8; len];
    if file.read_exact(&mut buffer).is_err() {
        return None;
    }
    let Ok((payload, _)) =
        bincode::decode_from_slice::<CachedChartPayload, _>(&buffer, bincode::config::standard())
    else {
        return None;
    };
    Some(payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_rules::timing::{
        FakeSegment, SpeedUnit, TimeSignatureSegment, TimingData, TimingSegments,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn cached_note_round_trips_to_parsed_note() {
        let source = ParsedNote {
            row_index: 12,
            column: 2,
            note_type: NoteType::Roll,
            tail_row_index: Some(36),
        };

        let cached = CachedParsedNote::from(&source);
        let parsed = ParsedNote::from(cached);

        assert_eq!(parsed, source);
    }

    #[test]
    fn cached_timing_round_trips_speed_and_signature_data() {
        let mut segments = TimingSegments::default();
        segments.beat0_offset_adjust = 0.25;
        segments.bpms = vec![(0.0, 120.0)];
        segments.speeds = vec![SpeedSegment {
            beat: 4.0,
            ratio: 2.0,
            delay: 0.5,
            unit: SpeedUnit::Seconds,
        }];
        segments.time_signatures = vec![TimeSignatureSegment {
            beat: 8.0,
            numerator: 3,
            denominator: 4,
        }];

        let round_trip = TimingSegments::from(CachedTimingSegments::from(&segments));

        assert_eq!(round_trip.beat0_offset_adjust, 0.25);
        assert_eq!(round_trip.bpms, vec![(0.0, 120.0)]);
        assert_eq!(round_trip.speeds[0].unit, SpeedUnit::Seconds);
        assert_eq!(round_trip.time_signatures[0].numerator, 3);
    }

    #[test]
    fn chart_display_bpm_parses_cache_shape() {
        assert!(matches!(
            parse_chart_display_bpm(Some("*")),
            Some(CachedChartDisplayBpm::Random)
        ));
        assert!(matches!(
            parse_chart_display_bpm(Some("240:120")),
            Some(CachedChartDisplayBpm::Specified { min, max })
                if min == 120.0 && max == 240.0
        ));
        assert!(parse_chart_display_bpm(Some("0")).is_none());
    }

    #[test]
    fn background_change_round_trips_path_target_and_fields() {
        let mut change = SongBackgroundChange::new(
            12.0,
            SongBackgroundChangeTarget::File(PathBuf::from("bg/movie.mpg")),
        );
        change.rate = 0.5;
        change.effect = "SongBgWithMovieViz".to_string();
        change.file2 = Some(PathBuf::from("bg/overlay.png"));
        change.transition = "FadeRight".to_string();
        change.color1 = Some([1.0, 0.0, 0.0, 1.0]);
        change.color2 = Some([0.0, 0.0, 1.0, 1.0]);

        let restored = SongBackgroundChange::from(SerializableSongBackgroundChange::from(&change));

        assert_eq!(restored.start_beat, change.start_beat);
        assert!(matches!(
            restored.target,
            SongBackgroundChangeTarget::File(ref path) if path == &PathBuf::from("bg/movie.mpg")
        ));
        assert_eq!(restored.rate, 0.5);
        assert_eq!(restored.effect, "SongBgWithMovieViz");
        assert_eq!(restored.file2, Some(PathBuf::from("bg/overlay.png")));
        assert_eq!(restored.transition, "FadeRight");
        assert_eq!(restored.color1, Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(restored.color2, Some([0.0, 0.0, 1.0, 1.0]));
    }

    #[test]
    fn chart_totals_skip_fake_and_unjudgable_notes() {
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 120.0)];
        segments.fakes = vec![FakeSegment {
            beat: 2.0,
            length: 1.0,
        }];
        let row_to_beat = vec![0.0, 1.0, 2.0, 3.0];
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &row_to_beat);
        let notes = vec![
            CachedParsedNote {
                row_index: 0,
                column: 0,
                note_type: CachedNoteType::Tap,
                tail_row_index: None,
            },
            CachedParsedNote {
                row_index: 1,
                column: 0,
                note_type: CachedNoteType::Hold,
                tail_row_index: Some(3),
            },
            CachedParsedNote {
                row_index: 2,
                column: 1,
                note_type: CachedNoteType::Mine,
                tail_row_index: None,
            },
            CachedParsedNote {
                row_index: 3,
                column: 1,
                note_type: CachedNoteType::Fake,
                tail_row_index: None,
            },
        ];

        assert_eq!(build_chart_totals(&notes, &timing), (15, 1, 0, 0));
    }

    #[test]
    fn measure_seconds_use_four_beat_boundaries() {
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 120.0)];
        let timing = TimingData::from_segments(0.0, 0.0, &segments, &[]);

        assert_eq!(build_measure_seconds(&timing, 3), vec![0.0, 2.0, 4.0]);
        assert!(!chart_has_attacks(Some("   ")));
        assert!(chart_has_attacks(Some("mod,0,1")));
    }

    #[test]
    fn gameplay_chart_payload_builds_timing_and_notes() {
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 120.0)];
        let payload = CachedChartPayload {
            notes: b"1000\n".to_vec(),
            parsed_notes: vec![CachedParsedNote {
                row_index: 0,
                column: 0,
                note_type: CachedNoteType::Tap,
                tail_row_index: None,
            }],
            row_to_beat: vec![0.0],
            timing_segments: CachedTimingSegments::from(&segments),
            chart_attacks: Some("mod,0,1".to_string()),
        };

        let chart = build_gameplay_chart_from_payload(payload, 0.25, 0.0);

        assert_eq!(chart.notes, b"1000\n");
        assert_eq!(chart.parsed_notes.len(), 1);
        assert_eq!(chart.parsed_notes[0].note_type, NoteType::Tap);
        assert_eq!(chart.row_to_beat, vec![0.0]);
        assert_eq!(chart.timing_segments.bpms, vec![(0.0, 120.0)]);
        assert_eq!(chart.chart_attacks.as_deref(), Some("mod,0,1"));
    }

    #[test]
    fn cached_chart_collection_loads_once_and_preserves_duplicates() {
        let payload = |notes: &[u8]| CachedChartPayload {
            notes: notes.to_vec(),
            parsed_notes: Vec::new(),
            row_to_beat: Vec::new(),
            timing_segments: CachedTimingSegments::from(&TimingSegments::default()),
            chart_attacks: None,
        };
        let source = vec![
            build_gameplay_chart_from_payload(payload(b"first"), 0.0, 0.0),
            build_gameplay_chart_from_payload(payload(b"second"), 0.0, 0.0),
        ];
        let requested = [1, 0, 1, 1];
        let mut loads = 0;

        let charts = collect_requested_cached_charts(&requested, |chart_ix| {
            loads += 1;
            source.get(chart_ix).cloned()
        })
        .unwrap();
        let legacy = collect_requested_cached_charts_legacy_for_bench(&source, &requested).unwrap();

        assert_eq!(loads, 2);
        assert_eq!(
            charts
                .iter()
                .map(|chart| chart.notes.as_slice())
                .collect::<Vec<_>>(),
            [b"second".as_slice(), b"first", b"second", b"second"]
        );
        assert_eq!(
            charts
                .iter()
                .map(|chart| chart.notes.as_slice())
                .collect::<Vec<_>>(),
            legacy
                .iter()
                .map(|chart| chart.notes.as_slice())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn requested_gameplay_charts_preserve_requested_order() {
        let mut song = cached_song(Path::new("song.sm"));
        let mut first = test_serializable_chart("dance-single", "Challenge", 0, None);
        first.notes = b"first".to_vec();
        let mut second = test_serializable_chart("dance-single", "Hard", 1, None);
        second.notes = b"second".to_vec();
        song.charts = vec![first, second];

        let legacy = build_requested_gameplay_charts_legacy(&song, &[1, 0], 0.0).unwrap();
        let charts = build_requested_gameplay_charts(&song, &[1, 0], 0.0).unwrap();

        assert_eq!(charts[0].notes, b"second");
        assert_eq!(charts[1].notes, b"first");
        for (chart, legacy) in charts.iter().zip(&legacy) {
            assert_eq!(chart.notes, legacy.notes);
            assert_eq!(chart.parsed_notes, legacy.parsed_notes);
            assert_eq!(chart.row_to_beat, legacy.row_to_beat);
            assert_eq!(chart.timing_segments.bpms, legacy.timing_segments.bpms);
            assert_eq!(chart.chart_attacks, legacy.chart_attacks);
        }
    }

    #[test]
    fn requested_gameplay_charts_reject_out_of_range_index() {
        let mut song = cached_song(Path::new("song.sm"));
        song.charts = vec![test_serializable_chart(
            "dance-single",
            "Challenge",
            0,
            None,
        )];

        assert_eq!(
            build_requested_gameplay_charts(&song, &[1], 0.0).unwrap_err(),
            "Chart index 1 out of range"
        );
    }

    #[test]
    fn precise_song_bounds_use_non_edit_playable_chart_rows() {
        let mut song = SerializableSongData {
            simfile_path: "song.sm".to_string(),
            title: String::new(),
            subtitle: String::new(),
            translit_title: String::new(),
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
            min_bpm: 60.0,
            max_bpm: 60.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 2,
            precise_last_second_seconds: 2.0,
            charts: vec![
                test_serializable_chart("dance-single", "Edit", 1, None),
                test_serializable_chart("lights-cabinet", "Challenge", 12, None),
                test_serializable_chart("dance-single", "Challenge", 4, Some(8)),
            ],
        };

        update_precise_song_bounds(&mut song, 0.0);

        assert_eq!(song.first_second, 4.0);
        assert_eq!(song.precise_last_second_seconds, 8.0);
    }

    #[test]
    fn borrowed_chart_payload_encoding_matches_owned_shape() {
        let mut song = cached_song(Path::new("song.ssc"));
        let mut chart = test_serializable_chart("dance-single", "Challenge", 4, Some(8));
        chart.notes = b"1000\n2000\n0000\n3000\n".to_vec();
        chart.chart_attacks = Some("mod,0,1".to_string());
        song.charts = vec![chart];

        assert_eq!(
            encode_chart_payloads_for_bench(&song),
            encode_chart_payloads_legacy_for_bench(&song)
        );
    }

    #[test]
    fn gameplay_cache_rejects_stale_directory_when_verifying() {
        let root = test_dir("gameplay-stale-directory-verified");
        let simfile = root.join("song.ssc");
        let cache_path = root.join("cache.bin");
        fs::write(&simfile, b"#TITLE:Old;").unwrap();
        write_song_cache_file(&cache_path, &cached_song(&simfile), 0.0).unwrap();

        fs::write(root.join("banner.png"), b"new asset").unwrap();

        assert!(load_cached_song_for_gameplay(&simfile, &cache_path, true).is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gameplay_cache_keeps_fastload_stale_directory_without_verifying() {
        let root = test_dir("gameplay-stale-directory-fastload");
        let simfile = root.join("song.ssc");
        let cache_path = root.join("cache.bin");
        fs::write(&simfile, b"#TITLE:Old;").unwrap();
        write_song_cache_file(&cache_path, &cached_song(&simfile), 0.0).unwrap();

        fs::write(root.join("banner.png"), b"new asset").unwrap();

        assert!(load_cached_song_for_gameplay(&simfile, &cache_path, false).is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn gameplay_chart_load_uses_cache_without_parse_fallback() {
        let root = test_dir("gameplay-load-cache-hit");
        let cache_dir = root.join("cache");
        let simfile = root.join("song.ssc");
        fs::write(&simfile, b"#TITLE:Cached;").unwrap();

        let mut data = cached_song(&simfile);
        let mut chart = test_serializable_chart("dance-single", "Challenge", 0, None);
        chart.notes = b"1000\n".to_vec();
        data.charts = vec![chart];
        let song = build_song_meta(data.clone(), 0.0);
        let cache_path = song_cache_path(&cache_dir, &simfile).unwrap();
        write_song_cache_file(&cache_path, &data, 0.0).unwrap();

        let parse_options = ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new());
        let options = GameplayChartLoadOptions {
            cache_dir: &cache_dir,
            parse_options: &parse_options,
            allow_cache_read: true,
            allow_cache_write: false,
            verify_cache_freshness: false,
            global_offset_seconds: 0.0,
        };

        let result = load_gameplay_charts_with_options(&song, &[0], &options, |_| {
            panic!("cache hit should not parse simfile")
        })
        .unwrap();

        assert_eq!(
            result.report.source,
            GameplayChartLoadSource::Cache {
                verify_freshness: false
            }
        );
        assert_eq!(result.report.requested_count, 1);
        assert!(result.report.song_data_load.is_none());
        assert!(result.report.warnings.is_empty());
        assert_eq!(result.charts[0].notes, b"1000\n");

        let _ = fs::remove_dir_all(root);
    }

    fn cached_song(path: &Path) -> SerializableSongData {
        SerializableSongData {
            simfile_path: path.to_string_lossy().into_owned(),
            title: "Cache Test".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
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
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-simfile-cache-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_serializable_chart(
        chart_type: &str,
        difficulty: &str,
        row_index: u32,
        tail_row_index: Option<u32>,
    ) -> SerializableChartData {
        let mut segments = TimingSegments::default();
        segments.bpms = vec![(0.0, 60.0)];
        let max_row = tail_row_index.unwrap_or(row_index) as usize;
        SerializableChartData {
            chart_type: chart_type.to_string(),
            difficulty: difficulty.to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 1,
            step_artist: String::new(),
            music_path: None,
            notes: Vec::new(),
            parsed_notes: vec![CachedParsedNote {
                row_index,
                column: 0,
                note_type: CachedNoteType::Tap,
                tail_row_index,
            }],
            row_to_beat: (0..=max_row).map(|row| row as f32).collect(),
            timing_segments: CachedTimingSegments::from(&segments),
            short_hash: String::new(),
            stats: CachedArrowStats {
                total_arrows: 0,
                left: 0,
                down: 0,
                up: 0,
                right: 0,
                total_steps: 0,
                jumps: 0,
                hands: 0,
                mines: 0,
                holds: 0,
                rolls: 0,
                lifts: 0,
                fakes: 0,
                holding: 0,
            },
            tech_counts: CachedTechCounts {
                crossovers: 0,
                half_crossovers: 0,
                full_crossovers: 0,
                footswitches: 0,
                up_footswitches: 0,
                down_footswitches: 0,
                sideswitches: 0,
                jacks: 0,
                brackets: 0,
                doublesteps: 0,
            },
            mines_nonfake: 0,
            stamina_counts: CachedStaminaCounts {
                anchors: 0,
                triangles: 0,
                boxes: 0,
                towers: 0,
                doritos: 0,
                hip_breakers: 0,
                copters: 0,
                spirals: 0,
                candles: 0,
                candle_percent: 0.0,
                staircases: 0,
                mono: 0,
                mono_percent: 0.0,
                sweeps: 0,
            },
            total_streams: 0,
            matrix_rating: 0.0,
            max_nps: 0.0,
            sn_detailed_breakdown: String::new(),
            sn_partial_breakdown: String::new(),
            sn_simple_breakdown: String::new(),
            detailed_breakdown: String::new(),
            partial_breakdown: String::new(),
            simple_breakdown: String::new(),
            chart_attacks: None,
            total_measures: 0,
            measure_nps_vec: Vec::new(),
            display_bpm: None,
            min_bpm: 60.0,
            max_bpm: 60.0,
        }
    }
}
