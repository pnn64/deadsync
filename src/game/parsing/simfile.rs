use crate::game::{
    chart::{ChartData, StaminaCounts},
    course::set_course_cache,
    note::NoteType,
    parsing::notes::ParsedNote,
    song::{SongData, SongPack, get_song_cache, set_song_cache},
    timing::{
        DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment, TimingData,
        TimingSegments, WarpSegment,
    },
};
use log::{info, warn};
use rssp::patterns::{PatternVariant, compute_box_counts, count_pattern};
use rssp::{AnalysisOptions, analyze};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bincode::{Decode, Encode};
use lewton::inside_ogg::OggStreamReader;
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;
use std::io::{BufReader, Cursor, Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};
use twox_hash::XxHash64;

const SONG_ANALYSIS_MONO_THRESHOLD: usize = 6;

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
    Fake,
}

impl From<NoteType> for CachedNoteType {
    fn from(note_type: NoteType) -> Self {
        match note_type {
            NoteType::Tap => Self::Tap,
            NoteType::Hold => Self::Hold,
            NoteType::Roll => Self::Roll,
            NoteType::Mine => Self::Mine,
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
struct SerializableChartData {
    chart_type: String,
    difficulty: String,
    description: String,
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
    max_nps: f64,
    sn_detailed_breakdown: String,
    sn_partial_breakdown: String,
    sn_simple_breakdown: String,
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
    chart_bpms: Option<String>,
    chart_stops: Option<String>,
    chart_delays: Option<String>,
    chart_warps: Option<String>,
    chart_speeds: Option<String>,
    chart_scrolls: Option<String>,
    chart_fakes: Option<String>,
    total_measures: usize,
    measure_nps_vec: Vec<f64>,
}

impl From<&ChartData> for SerializableChartData {
    fn from(chart: &ChartData) -> Self {
        Self {
            chart_type: chart.chart_type.clone(),
            difficulty: chart.difficulty.clone(),
            description: chart.description.clone(),
            meter: chart.meter,
            step_artist: chart.step_artist.clone(),
            notes: chart.notes.clone(),
            parsed_notes: chart
                .parsed_notes
                .iter()
                .map(CachedParsedNote::from)
                .collect(),
            row_to_beat: chart.row_to_beat.clone(),
            timing_segments: (&chart.timing_segments).into(),
            short_hash: chart.short_hash.clone(),
            stats: (&chart.stats).into(),
            tech_counts: (&chart.tech_counts).into(),
            mines_nonfake: chart.mines_nonfake,
            stamina_counts: (&chart.stamina_counts).into(),
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
            sn_detailed_breakdown: chart.sn_detailed_breakdown.clone(),
            sn_partial_breakdown: chart.sn_partial_breakdown.clone(),
            sn_simple_breakdown: chart.sn_simple_breakdown.clone(),
            detailed_breakdown: chart.detailed_breakdown.clone(),
            partial_breakdown: chart.partial_breakdown.clone(),
            simple_breakdown: chart.simple_breakdown.clone(),
            chart_bpms: chart.chart_bpms.clone(),
            chart_stops: chart.chart_stops.clone(),
            chart_delays: chart.chart_delays.clone(),
            chart_warps: chart.chart_warps.clone(),
            chart_speeds: chart.chart_speeds.clone(),
            chart_scrolls: chart.chart_scrolls.clone(),
            chart_fakes: chart.chart_fakes.clone(),
            total_measures: chart.total_measures,
            measure_nps_vec: chart.measure_nps_vec.clone(),
        }
    }
}

impl From<SerializableChartData> for ChartData {
    fn from(chart: SerializableChartData) -> Self {
        Self {
            chart_type: chart.chart_type,
            difficulty: chart.difficulty,
            description: chart.description,
            meter: chart.meter,
            step_artist: chart.step_artist,
            notes: chart.notes,
            parsed_notes: chart
                .parsed_notes
                .into_iter()
                .map(ParsedNote::from)
                .collect(),
            row_to_beat: chart.row_to_beat,
            timing_segments: chart.timing_segments.into(),
            timing: TimingData::default(),
            short_hash: chart.short_hash,
            stats: chart.stats.into(),
            tech_counts: chart.tech_counts.into(),
            mines_nonfake: chart.mines_nonfake,
            stamina_counts: chart.stamina_counts.into(),
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
            sn_detailed_breakdown: chart.sn_detailed_breakdown,
            sn_partial_breakdown: chart.sn_partial_breakdown,
            sn_simple_breakdown: chart.sn_simple_breakdown,
            detailed_breakdown: chart.detailed_breakdown,
            partial_breakdown: chart.partial_breakdown,
            simple_breakdown: chart.simple_breakdown,
            chart_bpms: chart.chart_bpms,
            chart_stops: chart.chart_stops,
            chart_delays: chart.chart_delays,
            chart_warps: chart.chart_warps,
            chart_speeds: chart.chart_speeds,
            chart_scrolls: chart.chart_scrolls,
            chart_fakes: chart.chart_fakes,
            total_measures: chart.total_measures,
            measure_nps_vec: chart.measure_nps_vec,
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
    cdtitle_path: Option<String>,
    music_path: Option<String>,
    display_bpm: String,
    offset: f32,
    sample_start: Option<f32>,
    sample_length: Option<f32>,
    min_bpm: f64,
    max_bpm: f64,
    normalized_bpms: String,
    normalized_stops: String,
    normalized_delays: String,
    normalized_warps: String,
    normalized_speeds: String,
    normalized_scrolls: String,
    normalized_fakes: String,
    music_length_seconds: f32,
    total_length_seconds: i32,
    charts: Vec<SerializableChartData>,
}

impl From<&SongData> for SerializableSongData {
    fn from(song: &SongData) -> Self {
        Self {
            simfile_path: song.simfile_path.to_string_lossy().into_owned(),
            title: song.title.clone(),
            subtitle: song.subtitle.clone(),
            translit_title: song.translit_title.clone(),
            translit_subtitle: song.translit_subtitle.clone(),
            artist: song.artist.clone(),
            banner_path: song
                .banner_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            background_path: song
                .background_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            cdtitle_path: song
                .cdtitle_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            music_path: song
                .music_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            display_bpm: song.display_bpm.clone(),
            offset: song.offset,
            sample_start: song.sample_start,
            sample_length: song.sample_length,
            min_bpm: song.min_bpm,
            max_bpm: song.max_bpm,
            normalized_bpms: song.normalized_bpms.clone(),
            normalized_stops: song.normalized_stops.clone(),
            normalized_delays: song.normalized_delays.clone(),
            normalized_warps: song.normalized_warps.clone(),
            normalized_speeds: song.normalized_speeds.clone(),
            normalized_scrolls: song.normalized_scrolls.clone(),
            normalized_fakes: song.normalized_fakes.clone(),
            music_length_seconds: song.music_length_seconds,
            total_length_seconds: song.total_length_seconds,
            charts: song
                .charts
                .iter()
                .map(SerializableChartData::from)
                .collect(),
        }
    }
}

impl From<SerializableSongData> for SongData {
    fn from(song: SerializableSongData) -> Self {
        Self {
            simfile_path: PathBuf::from(song.simfile_path),
            title: song.title,
            subtitle: song.subtitle,
            translit_title: song.translit_title,
            translit_subtitle: song.translit_subtitle,
            artist: song.artist,
            banner_path: song.banner_path.map(PathBuf::from),
            background_path: song.background_path.map(PathBuf::from),
            cdtitle_path: song.cdtitle_path.map(PathBuf::from),
            music_path: song.music_path.map(PathBuf::from),
            display_bpm: song.display_bpm,
            offset: song.offset,
            sample_start: song.sample_start,
            sample_length: song.sample_length,
            min_bpm: song.min_bpm,
            max_bpm: song.max_bpm,
            normalized_bpms: song.normalized_bpms,
            normalized_stops: song.normalized_stops,
            normalized_delays: song.normalized_delays,
            normalized_warps: song.normalized_warps,
            normalized_speeds: song.normalized_speeds,
            normalized_scrolls: song.normalized_scrolls,
            normalized_fakes: song.normalized_fakes,
            music_length_seconds: song.music_length_seconds,
            total_length_seconds: song.total_length_seconds,
            charts: song.charts.into_iter().map(ChartData::from).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Encode, Decode)]
struct CachedSong {
    rssp_version: String,
    mono_threshold: usize,
    source_hash: u64,
    data: SerializableSongData,
}

// --- CACHING HELPER FUNCTIONS ---

#[derive(Clone)]
struct SongCacheKeys {
    cache_path: Option<PathBuf>,
}

fn get_content_hash(path: &Path) -> Result<u64, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = XxHash64::with_seed(0);
    // Using a buffer is much more memory-efficient than reading the whole file at once.
    let mut buffer = [0; 8192];
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.write(&buffer[..bytes_read]);
    }
    Ok(hasher.finish())
}

fn get_cache_path(simfile_path: &Path) -> Result<PathBuf, std::io::Error> {
    let canonical_path = simfile_path.canonicalize()?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical_path.to_string_lossy().as_bytes());
    let path_hash = hasher.finish();

    let cache_dir = Path::new("cache/songs");
    let hash_hex = format!("{path_hash:016x}");
    let shard2 = &hash_hex[..2];
    Ok(cache_dir.join(shard2).join(format!("{hash_hex}.bin")))
}

fn compute_song_cache_keys(path: &Path) -> SongCacheKeys {
    let cache_path = match get_cache_path(path) {
        Ok(p) => Some(p),
        Err(e) => {
            warn!(
                "Could not generate cache path for {path:?}: {e}. Caching disabled for this file."
            );
            None
        }
    };
    SongCacheKeys { cache_path }
}

fn fmt_scan_time(d: Duration) -> String {
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

// Mirrors ITGmania's `SongUtil::MakeSortString` behavior for song title sorting.
fn itgmania_make_sort_bytes(s: &str) -> Vec<u8> {
    let mut out = s.as_bytes().to_vec();
    out.make_ascii_uppercase();

    if matches!(out.first(), Some(b'.')) {
        out.remove(0);
    }

    if let Some(&b) = out.first() {
        let is_alpha = b.is_ascii_uppercase();
        let is_digit = b.is_ascii_digit();
        if !is_alpha && !is_digit {
            out.insert(0, b'~');
        }
    }

    out
}

// Mirrors ITGmania's `CompareSongPointersByTitle` (translit main title, then translit subtitle,
// then case-insensitive song file path for deterministic ordering).
struct ItgmaniaSongTitleKey {
    main_raw: Vec<u8>,
    main_sort: Vec<u8>,
    sub_sort: Vec<u8>,
    path_fold: Vec<u8>,
}

impl ItgmaniaSongTitleKey {
    fn new(song: &SongData) -> Self {
        let main_raw_str = if song.translit_title.is_empty() {
            song.title.as_str()
        } else {
            song.translit_title.as_str()
        };
        let sub_raw_str = if song.translit_subtitle.is_empty() {
            song.subtitle.as_str()
        } else {
            song.translit_subtitle.as_str()
        };

        let mut path_fold = song
            .simfile_path
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        path_fold.make_ascii_lowercase();

        Self {
            main_raw: main_raw_str.as_bytes().to_vec(),
            main_sort: itgmania_make_sort_bytes(main_raw_str),
            sub_sort: itgmania_make_sort_bytes(sub_raw_str),
            path_fold,
        }
    }
}

impl PartialEq for ItgmaniaSongTitleKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for ItgmaniaSongTitleKey {}

impl PartialOrd for ItgmaniaSongTitleKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ItgmaniaSongTitleKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.main_raw == other.main_raw {
            match self.sub_sort.cmp(&other.sub_sort) {
                std::cmp::Ordering::Equal => self.path_fold.cmp(&other.path_fold),
                o => o,
            }
        } else {
            match self.main_sort.cmp(&other.main_sort) {
                std::cmp::Ordering::Equal => self.path_fold.cmp(&other.path_fold),
                o => o,
            }
        }
    }
}

fn step_type_lanes(step_type: &str) -> usize {
    let normalized = step_type.trim().to_ascii_lowercase().replace('_', "-");
    if normalized == "dance-double" { 8 } else { 4 }
}

fn hydrate_chart_timings(song: &mut SongData, global_offset_seconds: f32) {
    let song_offset = song.offset;

    for chart in &mut song.charts {
        chart.timing = TimingData::from_segments(
            -song_offset,
            global_offset_seconds,
            &chart.timing_segments,
            &chart.row_to_beat,
        );
    }
}

/// Helper to load a song from cache OR parse it if needed.
/// Returns (`SongData`, `is_cache_hit`).
fn process_song(
    simfile_path: PathBuf,
    fastload: bool,
    cachesongs: bool,
    global_offset_seconds: f32,
) -> Result<(SongData, bool), String> {
    let cache_keys = if fastload || cachesongs {
        compute_song_cache_keys(&simfile_path)
    } else {
        SongCacheKeys { cache_path: None }
    };

    // 1. Try Loading from Cache
    if fastload
        && let Some(cp) = &cache_keys.cache_path
        && let Some(song_data) = load_song_from_cache(&simfile_path, cp, global_offset_seconds)
    {
        return Ok((song_data, true)); // is_hit = true
    }

    // 2. Parse from Source (Cache Miss)
    let song_data = parse_song_and_maybe_write_cache(
        &simfile_path,
        fastload,
        cachesongs,
        cache_keys,
        global_offset_seconds,
    )?;
    Ok((song_data, false)) // is_hit = false
}

/// Scans the provided root directory (e.g., "songs/") for simfiles,
/// parses them, and populates the global cache. This should be run once at startup.
pub fn scan_and_load_songs(root_path_str: &'static str) {
    scan_and_load_songs_impl::<fn(&str, &str)>(root_path_str, None);
}

pub fn scan_and_load_songs_with_progress<F>(
    root_path_str: &'static str,
    progress: &mut F,
)
where
    F: FnMut(&str, &str),
{
    scan_and_load_songs_impl(root_path_str, Some(progress));
}

fn scan_and_load_songs_impl<F>(
    root_path_str: &'static str,
    mut progress: Option<&mut F>,
)
where
    F: FnMut(&str, &str),
{
    info!("Starting simfile scan in '{root_path_str}'...");

    let started = Instant::now();
    let config = crate::config::get();
    let fastload = config.fastload;
    let cachesongs = config.cachesongs;
    let global_offset_seconds = config.global_offset_seconds;

    let avail_threads = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let mut parse_threads = match config.song_parsing_threads {
        0 => avail_threads,
        1 => 1,
        n => (n as usize).min(avail_threads).max(1),
    };
    if parse_threads < 1 {
        parse_threads = 1;
    }
    let parallel_parsing = parse_threads > 1;

    // Ensure the cache directory exists before we start scanning.
    let cache_dir = Path::new("cache/songs");
    if let Err(e) = fs::create_dir_all(cache_dir) {
        warn!(
            "Could not create cache directory '{}': {}. Caching will be disabled.",
            cache_dir.to_string_lossy(),
            e
        );
    }

    let root_path = Path::new(root_path_str);
    if !root_path.exists() || !root_path.is_dir() {
        warn!("Songs directory '{root_path_str}' not found. No songs will be loaded.");
        return;
    }

    let mut loaded_packs = Vec::new();
    let mut songs_cache_hits = 0usize;
    let mut songs_parsed = 0usize;
    let mut songs_failed = 0usize;

    let packs = match rssp::pack::scan_songs_dir(root_path, rssp::pack::ScanOpt::default()) {
        Ok(p) => p,
        Err(e) => {
            warn!("Could not scan songs dir '{root_path_str}': {e:?}");
            return;
        }
    };

    // (pack_idx, simfile_path, result(song_data, is_cache_hit))
    type ParseMsg = (usize, PathBuf, Result<(Arc<SongData>, bool), String>);

    let mut runtime: Option<tokio::runtime::Runtime> = None;
    let mut tx_opt: Option<std::sync::mpsc::Sender<ParseMsg>> = None;
    let mut rx_opt: Option<std::sync::mpsc::Receiver<ParseMsg>> = None;
    let mut in_flight = 0usize;

    fn reap_one(
        rx: Option<&std::sync::mpsc::Receiver<ParseMsg>>,
        in_flight: &mut usize,
        loaded_packs: &mut Vec<SongPack>,
        songs_failed: &mut usize,
        songs_cache_hits: &mut usize,
        songs_parsed: &mut usize,
    ) {
        let Some(rx) = rx else {
            return;
        };
        match rx.recv() {
            Ok((pack_idx, simfile_path, result)) => {
                *in_flight = in_flight.saturating_sub(1);
                match result {
                    Ok((song_data, is_hit)) => {
                        if is_hit {
                            *songs_cache_hits += 1;
                        } else {
                            *songs_parsed += 1;
                        }
                        if let Some(pack) = loaded_packs.get_mut(pack_idx) {
                            pack.songs.push(song_data);
                        }
                    }
                    Err(e) => {
                        *songs_failed += 1;
                        warn!("Failed to load '{simfile_path:?}': {e}")
                    }
                }
            }
            Err(_) => {
                *in_flight = 0;
            }
        }
    }

    for pack in packs {
        let pack_display = pack
            .dir
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(pack.group_name.as_str())
            .to_owned();

        let current_pack = SongPack {
            group_name: pack.group_name,
            name: pack.display_title,
            sort_title: pack.sort_title,
            translit_title: pack.translit_title,
            series: pack.series,
            year: pack.year,
            sync_pref: pack.sync_pref,
            directory: pack.dir,
            banner_path: pack.banner_path,
            songs: Vec::new(),
        };
        info!("Scanning pack: {}", current_pack.name);
        let pack_idx = loaded_packs.len();
        loaded_packs.push(current_pack);

        for song in pack.songs {
            let simfile_path = song.simfile;
            if let Some(cb) = progress.as_mut() {
                let song_display = simfile_path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| {
                        simfile_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or_default()
                    });
                cb(pack_display.as_str(), song_display);
            }

            if parallel_parsing {
                let rt = runtime.get_or_insert_with(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .max_blocking_threads(parse_threads)
                        .build()
                        .unwrap()
                });
                if tx_opt.is_none() || rx_opt.is_none() {
                    let (tx, rx) = std::sync::mpsc::channel::<ParseMsg>();
                    tx_opt = Some(tx);
                    rx_opt = Some(rx);
                }

                while in_flight >= parse_threads {
                    reap_one(
                        rx_opt.as_ref(),
                        &mut in_flight,
                        &mut loaded_packs,
                        &mut songs_failed,
                        &mut songs_cache_hits,
                        &mut songs_parsed,
                    );
                }

                let Some(tx) = tx_opt.as_ref() else {
                    // Fallback to sync if channel creation failed (unlikely)
                    match process_song(
                        simfile_path.clone(),
                        fastload,
                        cachesongs,
                        global_offset_seconds,
                    ) {
                        Ok((song_data, is_hit)) => {
                            if is_hit {
                                songs_cache_hits += 1;
                            } else {
                                songs_parsed += 1;
                            }
                            loaded_packs[pack_idx].songs.push(Arc::new(song_data));
                        }
                        Err(e) => {
                            songs_failed += 1;
                            warn!("Failed to load '{simfile_path:?}': {e}")
                        }
                    }
                    continue;
                };

                let tx = tx.clone();
                let simfile_path_owned = simfile_path.clone();
                rt.handle().spawn_blocking(move || {
                    let out = catch_unwind(AssertUnwindSafe(|| {
                        process_song(
                            simfile_path_owned.clone(),
                            fastload,
                            cachesongs,
                            global_offset_seconds,
                        )
                        .map(|(d, h)| (Arc::new(d), h))
                    }))
                    .unwrap_or_else(|_| Err("Song parse panicked".to_string()));
                    let _ = tx.send((pack_idx, simfile_path_owned, out));
                });
                in_flight += 1;
            } else {
                match process_song(
                    simfile_path.clone(),
                    fastload,
                    cachesongs,
                    global_offset_seconds,
                ) {
                    Ok((song_data, is_hit)) => {
                        if is_hit {
                            songs_cache_hits += 1;
                        } else {
                            songs_parsed += 1;
                        }
                        loaded_packs[pack_idx].songs.push(Arc::new(song_data));
                    }
                    Err(e) => {
                        songs_failed += 1;
                        warn!("Failed to load '{simfile_path:?}': {e}")
                    }
                }
            }
        }
    }

    while in_flight > 0 {
        reap_one(
            rx_opt.as_ref(),
            &mut in_flight,
            &mut loaded_packs,
            &mut songs_failed,
            &mut songs_cache_hits,
            &mut songs_parsed,
        );
    }

    if runtime.is_some() {
        info!(
            "Song parsing: used {} threads for cache/parsing (SongParsingThreads={}).",
            parse_threads, config.song_parsing_threads
        );
    }

    loaded_packs.retain(|p| !p.songs.is_empty());
    for pack in &mut loaded_packs {
        pack.songs
            .sort_by_cached_key(|s| ItgmaniaSongTitleKey::new(s.as_ref()));
    }

    loaded_packs.sort_by_cached_key(|p| {
        (
            p.sort_title.to_ascii_lowercase(),
            p.group_name.to_ascii_lowercase(),
        )
    });

    let songs_loaded = loaded_packs.iter().map(|p| p.songs.len()).sum::<usize>();
    info!(
        "Finished scan. Found {} packs / {} songs (parsed {}, cache hits {}, failed {}) in {}.",
        loaded_packs.len(),
        songs_loaded,
        songs_parsed,
        songs_cache_hits,
        songs_failed,
        fmt_scan_time(started.elapsed())
    );
    set_song_cache(loaded_packs);
}

fn is_dir_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    let want = name.trim().to_ascii_lowercase();
    if want.is_empty() {
        return None;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let got = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if got == want {
            return Some(entry.path());
        }
    }
    None
}

fn collect_course_paths(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("crs"))
            {
                out.push(path);
            }
        }
    }
    out.sort_by_cached_key(|p| p.to_string_lossy().to_ascii_lowercase());
    out
}

fn resolve_song_dir(
    songs_dir: &Path,
    group_dirs: &mut HashMap<String, PathBuf>,
    group: Option<&str>,
    song: &str,
) -> Option<PathBuf> {
    fn resolve_group_dir(
        songs_dir: &Path,
        group_dirs: &mut HashMap<String, PathBuf>,
        group: &str,
    ) -> Option<PathBuf> {
        let key = group.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        if !group_dirs.contains_key(&key) {
            let direct = songs_dir.join(group);
            let path = if direct.is_dir() {
                Some(direct)
            } else {
                is_dir_ci(songs_dir, group)
            }?;
            group_dirs.insert(key.clone(), path);
        }
        group_dirs.get(&key).cloned()
    }

    let song = song.trim();
    if song.is_empty() {
        return None;
    }

    if let Some(group) = group.map(str::trim).filter(|g| !g.is_empty()) {
        let group_dir = resolve_group_dir(songs_dir, group_dirs, group)?;
        let direct = group_dir.join(song);
        return if direct.is_dir() {
            Some(direct)
        } else {
            is_dir_ci(&group_dir, song)
        };
    }

    let Ok(entries) = fs::read_dir(songs_dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let group_dir = entry.path();
        if !group_dir.is_dir() {
            continue;
        }
        let direct = group_dir.join(song);
        if direct.is_dir() {
            return Some(direct);
        }
        if let Some(found) = is_dir_ci(&group_dir, song) {
            return Some(found);
        }
    }
    None
}

fn resolve_course_group_dir(
    songs_dir: &Path,
    group_dirs: &mut HashMap<String, PathBuf>,
    group: &str,
) -> Option<PathBuf> {
    let key = group.trim().to_ascii_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Some(path) = group_dirs.get(&key) {
        return Some(path.clone());
    }
    let direct = songs_dir.join(group);
    let path = if direct.is_dir() {
        Some(direct)
    } else {
        is_dir_ci(songs_dir, group)
    }?;
    group_dirs.insert(key, path.clone());
    Some(path)
}

fn autogen_nonstop_group_courses() -> Vec<(PathBuf, rssp::course::CourseFile)> {
    let song_cache = get_song_cache();
    let mut out = Vec::with_capacity(song_cache.len());

    for pack in song_cache.iter() {
        if pack.songs.is_empty() {
            continue;
        }

        let group_name = pack.group_name.trim();
        if group_name.is_empty() {
            continue;
        }
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };

        let mut entries = Vec::with_capacity(4);
        for _ in 0..4 {
            entries.push(rssp::course::CourseEntry {
                song: rssp::course::CourseSong::RandomWithinGroup {
                    group: group_name.to_string(),
                },
                steps: rssp::course::StepsSpec::Difficulty(rssp::course::Difficulty::Medium),
                modifiers: String::new(),
                secret: true,
                no_difficult: false,
                gain_lives: -1,
            });
        }

        let mut path = PathBuf::from("courses");
        path.push(group_name);
        path.push("__deadsync_autogen_nonstop_random.crs");

        out.push((
            path,
            rssp::course::CourseFile {
                name: format!("{display_name} Random"),
                name_translit: String::new(),
                scripter: "Autogen".to_string(),
                description: String::new(),
                banner: pack
                    .banner_path
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                background: String::new(),
                repeat: false,
                lives: -1,
                meters: [None; 6],
                entries,
            },
        ));
    }

    out
}

pub fn scan_and_load_courses(courses_root_str: &'static str, songs_root_str: &'static str) {
    scan_and_load_courses_impl::<fn(&str, &str)>(courses_root_str, songs_root_str, None);
}

pub fn scan_and_load_courses_with_progress<F>(
    courses_root_str: &'static str,
    songs_root_str: &'static str,
    progress: &mut F,
)
where
    F: FnMut(&str, &str),
{
    scan_and_load_courses_impl(courses_root_str, songs_root_str, Some(progress));
}

fn scan_and_load_courses_impl<F>(
    courses_root_str: &'static str,
    songs_root_str: &'static str,
    mut progress: Option<&mut F>,
)
where
    F: FnMut(&str, &str),
{
    info!("Starting course scan in '{courses_root_str}'...");
    let started = Instant::now();

    let courses_root = Path::new(courses_root_str);
    if !courses_root.is_dir() {
        warn!("Courses directory '{courses_root_str}' not found. No courses will be loaded.");
        set_course_cache(Vec::new());
        return;
    }

    let songs_root = Path::new(songs_root_str);
    if !songs_root.is_dir() {
        warn!("Songs directory '{songs_root_str}' not found. No courses will be loaded.");
        set_course_cache(Vec::new());
        return;
    }

    let mut loaded_courses = Vec::new();
    let mut courses_failed = 0usize;
    let mut group_dirs: HashMap<String, PathBuf> = HashMap::new();
    let total_song_count = {
        let song_cache = get_song_cache();
        song_cache
            .iter()
            .map(|pack| pack.songs.len())
            .sum::<usize>()
    };

    for course_path in collect_course_paths(courses_root) {
        if let Some(cb) = progress.as_mut() {
            let group_display = course_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or(courses_root_str);
            let course_display = course_path
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty())
                .unwrap_or_default();
            cb(group_display, course_display);
        }
        let data = match fs::read(&course_path) {
            Ok(d) => d,
            Err(e) => {
                courses_failed += 1;
                warn!("Failed to read course '{}': {}", course_path.display(), e);
                continue;
            }
        };

        let course = match rssp::course::parse_crs(&data) {
            Ok(c) => c,
            Err(e) => {
                courses_failed += 1;
                warn!("Failed to parse course '{}': {}", course_path.display(), e);
                continue;
            }
        };

        let mut ok = true;
        for (idx, entry) in course.entries.iter().enumerate() {
            match &entry.song {
                rssp::course::CourseSong::Fixed { group, song } => {
                    let Some(song_dir) =
                        resolve_song_dir(songs_root, &mut group_dirs, group.as_deref(), song)
                    else {
                        warn!(
                            "Course '{}' entry {} references missing song '{}{}'.",
                            course.name,
                            idx + 1,
                            group
                                .as_deref()
                                .map(|g| format!("{g}/"))
                                .unwrap_or_default(),
                            song
                        );
                        ok = false;
                        break;
                    };

                    match rssp::pack::scan_song_dir(&song_dir, rssp::pack::ScanOpt::default()) {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            warn!(
                                "Course '{}' entry {} song dir has no simfile: {}",
                                course.name,
                                idx + 1,
                                song_dir.display()
                            );
                            ok = false;
                            break;
                        }
                        Err(e) => {
                            warn!(
                                "Course '{}' entry {} failed scanning song dir {}: {e:?}",
                                course.name,
                                idx + 1,
                                song_dir.display()
                            );
                            ok = false;
                            break;
                        }
                    }
                }
                rssp::course::CourseSong::SortPick { sort, index } => {
                    let supports_sort = matches!(
                        sort,
                        rssp::course::SongSort::MostPlays | rssp::course::SongSort::FewestPlays
                    );
                    if !supports_sort {
                        warn!(
                            "Course '{}' has unsupported sort selector in entry {} ({sort:?}).",
                            course.name,
                            idx + 1,
                        );
                        ok = false;
                        break;
                    }

                    let choose_index = (*index).max(0) as usize;
                    if choose_index >= total_song_count {
                        let label = match sort {
                            rssp::course::SongSort::MostPlays => "BEST",
                            rssp::course::SongSort::FewestPlays => "WORST",
                            rssp::course::SongSort::TopGrades => "GRADEBEST",
                            rssp::course::SongSort::LowestGrades => "GRADEWORST",
                        };
                        warn!(
                            "Course '{}' entry {} references out-of-range sort pick '{}{}' with only {} songs installed.",
                            course.name,
                            idx + 1,
                            label,
                            choose_index.saturating_add(1),
                            total_song_count
                        );
                        ok = false;
                        break;
                    }
                }
                rssp::course::CourseSong::RandomAny => {}
                rssp::course::CourseSong::RandomWithinGroup { group } => {
                    if resolve_course_group_dir(songs_root, &mut group_dirs, group).is_none() {
                        warn!(
                            "Course '{}' entry {} references missing group '{}/*'.",
                            course.name,
                            idx + 1,
                            group
                        );
                        ok = false;
                        break;
                    }
                }
                _ => {
                    warn!(
                        "Course '{}' has unsupported song selector in entry {}.",
                        course.name,
                        idx + 1
                    );
                    ok = false;
                    break;
                }
            }
        }

        if ok {
            loaded_courses.push((course_path, course));
        } else {
            courses_failed += 1;
        }
    }

    let autogen_courses = autogen_nonstop_group_courses();
    let autogen_count = autogen_courses.len();
    loaded_courses.extend(autogen_courses);

    info!(
        "Finished course scan. Loaded {} courses ({} autogen, failed {}) in {}.",
        loaded_courses.len(),
        autogen_count,
        courses_failed,
        fmt_scan_time(started.elapsed())
    );
    set_course_cache(loaded_courses);
}

fn load_song_from_cache(
    path: &Path,
    cache_path: &Path,
    global_offset_seconds: f32,
) -> Option<SongData> {
    if !cache_path.exists() {
        return None;
    }
    let Ok(mut file) = fs::File::open(cache_path) else {
        return None;
    };
    let mut buffer = Vec::new();
    if file.read_to_end(&mut buffer).is_err() {
        return None;
    }
    let Ok((cached_song, _)) =
        bincode::decode_from_slice::<CachedSong, _>(&buffer, bincode::config::standard())
    else {
        return None;
    };

    if cached_song.rssp_version != rssp::RSSP_VERSION {
        info!(
            "Cache stale (rssp version mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }
    if cached_song.mono_threshold != SONG_ANALYSIS_MONO_THRESHOLD {
        info!(
            "Cache stale (mono threshold mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }

    let content_hash = match get_content_hash(path) {
        Ok(h) => h,
        Err(e) => {
            warn!(
                "Could not hash content of {:?}: {}. Ignoring cache.",
                path.file_name().unwrap_or_default(),
                e
            );
            return None;
        }
    };

    if cached_song.source_hash != content_hash {
        info!(
            "Cache stale (content hash mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }

    info!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
    let mut song_data: SongData = cached_song.data.into();
    hydrate_chart_timings(&mut song_data, global_offset_seconds);
    Some(song_data)
}

fn parse_song_and_maybe_write_cache(
    path: &Path,
    fastload: bool,
    cachesongs: bool,
    cache_keys: SongCacheKeys,
    global_offset_seconds: f32,
) -> Result<SongData, String> {
    if fastload {
        info!("Cache miss for: {:?}", path.file_name().unwrap_or_default());
    } else {
        info!(
            "Parsing (fastload disabled): {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    let need_hash = cachesongs && cache_keys.cache_path.is_some();
    let (song_data, content_hash) = parse_and_process_song_file(path, need_hash)?;

    if cachesongs && let (Some(cp), Some(ch)) = (cache_keys.cache_path, content_hash) {
        let serializable_data: SerializableSongData = (&song_data).into();
        let cached_song = CachedSong {
            rssp_version: rssp::RSSP_VERSION.to_string(),
            mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
            source_hash: ch,
            data: serializable_data,
        };

        if let Ok(encoded) = bincode::encode_to_vec(&cached_song, bincode::config::standard()) {
            let mut can_write = true;
            if let Some(parent) = cp.parent()
                && let Err(e) = fs::create_dir_all(parent)
            {
                warn!("Failed to create song cache dir {parent:?}: {e}");
                can_write = false;
            }
            if can_write {
                if let Ok(mut file) = fs::File::create(&cp) {
                    if file.write_all(&encoded).is_err() {
                        warn!("Failed to write cache file for {cp:?}");
                    }
                } else {
                    warn!("Failed to create cache file for {cp:?}");
                }
            }
        }
    }

    let mut song_data = song_data;
    hydrate_chart_timings(&mut song_data, global_offset_seconds);
    Ok(song_data)
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
fn resolve_song_asset_path_like_itg(song_dir: &Path, asset_tag: &str) -> Option<PathBuf> {
    let asset_tag = asset_tag.trim();
    if asset_tag.is_empty() {
        return None;
    }

    let asset_tag_slash = asset_tag.replace('\\', "/");
    let rel_path = song_dir.join(&asset_tag_slash);
    if rel_path.is_file() {
        return Some(rel_path);
    }
    if !asset_tag_slash.contains('/') {
        return None;
    }

    let mut song_dir_slash = song_dir.to_string_lossy().replace('\\', "/");
    if !song_dir_slash.ends_with('/') {
        song_dir_slash.push('/');
    }

    let collapsed = if asset_tag_slash.starts_with("../") {
        collapse_song_asset_path(&(song_dir_slash + asset_tag_slash.as_str()))
    } else {
        collapse_song_asset_path(&asset_tag_slash)
    };
    if collapsed.starts_with("../") {
        return None;
    }

    let collapsed_path = PathBuf::from(collapsed);
    collapsed_path.is_file().then_some(collapsed_path)
}

/// The original parsing logic, now separated to be called on a cache miss.
fn parse_and_process_song_file(
    path: &Path,
    need_hash: bool,
) -> Result<(SongData, Option<u64>), String> {
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
    let charts: Vec<ChartData> = summary
        .charts
        .into_iter()
        .map(|c| {
            let lanes = step_type_lanes(&c.step_type_str);
            let parsed_notes =
                crate::game::parsing::notes::parse_chart_notes(&c.minimized_note_data, lanes);
            let stamina_counts = build_stamina_counts(&c);
            info!(
                "  Chart '{}' [{}] loaded with {} bytes of note data.",
                c.difficulty_str,
                c.rating_str,
                c.minimized_note_data.len()
            );
            ChartData {
                chart_type: c.step_type_str,
                difficulty: c.difficulty_str,
                description: c.description_str,
                meter: c.rating_str.parse().unwrap_or(0),
                step_artist: c.step_artist_str,
                notes: c.minimized_note_data,
                parsed_notes,
                row_to_beat: c.row_to_beat,
                timing_segments: TimingSegments::from(&c.timing_segments),
                timing: TimingData::default(),
                short_hash: c.short_hash,
                stats: c.stats,
                tech_counts: c.tech_counts,
                mines_nonfake: c.mines_nonfake,
                stamina_counts,
                total_streams: c.total_streams,
                total_measures: c.total_measures,
                max_nps: c.max_nps,
                sn_detailed_breakdown: c.sn_detailed_breakdown,
                sn_partial_breakdown: c.sn_partial_breakdown,
                sn_simple_breakdown: c.sn_simple_breakdown,
                detailed_breakdown: c.detailed_breakdown,
                partial_breakdown: c.partial_breakdown,
                simple_breakdown: c.simple_breakdown,
                measure_nps_vec: c.measure_nps_vec,
                chart_bpms: c.chart_bpms,
                chart_stops: c.chart_stops,
                chart_delays: c.chart_delays,
                chart_warps: c.chart_warps,
                chart_speeds: c.chart_speeds,
                chart_scrolls: c.chart_scrolls,
                chart_fakes: c.chart_fakes,
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
    let cdtitle_path = resolve_song_asset_path_like_itg(simfile_dir, &summary.cdtitle_path);

    let music_path = if summary.music_path.is_empty() {
        None
    } else {
        Some(simfile_dir.join(summary.music_path))
    };

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
        SongData {
            simfile_path: path.to_path_buf(),
            title: summary.title_str,
            subtitle: summary.subtitle_str,
            translit_title: summary.titletranslit_str,
            translit_subtitle: summary.subtitletranslit_str,
            artist: summary.artist_str,
            banner_path, // Keep original logic for banner
            background_path: background_path_opt,
            cdtitle_path,
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
            normalized_stops: summary.normalized_stops,
            normalized_delays: summary.normalized_delays,
            normalized_warps: summary.normalized_warps,
            normalized_speeds: summary.normalized_speeds,
            normalized_scrolls: summary.normalized_scrolls,
            normalized_fakes: summary.normalized_fakes,
            music_path,
            music_length_seconds,
            total_length_seconds: summary.total_length,
            charts,
        },
        content_hash,
    ))
}

/// Computes the length of the music file in seconds, if it is a readable OGG file.
/// Returns 0.0 on failure or if no music path is provided.
fn compute_music_length_seconds(music_path: Option<&Path>) -> f32 {
    let Some(path) = music_path else {
        return 0.0;
    };

    let ext_is_ogg = path
        .extension()
        .and_then(|s| s.to_str())
        .is_some_and(|ext| {
            let ext_lower = ext.to_ascii_lowercase();
            ext_lower == "ogg" || ext_lower == "oga"
        });
    if !ext_is_ogg {
        return 0.0;
    }

    match ogg_length_seconds(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute OGG length for {path:?}: {e}");
            0.0
        }
    }
}

/// Fast OGG length detection: use lewton only for headers (sample rate) and
/// scan backwards through the file to find the last valid granule position.
fn ogg_length_seconds(path: &Path) -> Result<f32, String> {
    let file = fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;

    // Safe wrapper around the unsafe memmap2 API.
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("Memory-map failed: {e}"))?;

    let sample_rate_hz = ogg_sample_rate_hz(&mmap)?;

    let total_samples = find_last_granule_backwards(&mmap)?;
    Ok((total_samples as f64 / sample_rate_hz) as f32)
}

fn ogg_sample_rate_hz(data: &[u8]) -> Result<f64, String> {
    match ogg_sample_rate_hz_lewton(data) {
        Ok(rate) => Ok(rate),
        Err(lewton_err) => ogg_sample_rate_hz_ident_packet(data).ok_or_else(|| {
            format!("lewton header error: {lewton_err}; fallback OGG ident parse failed")
        }),
    }
}

fn ogg_sample_rate_hz_lewton(data: &[u8]) -> Result<f64, String> {
    let cursor = Cursor::new(data);
    let reader = OggStreamReader::new(BufReader::new(cursor)).map_err(|e| format!("{e}"))?;
    let rate = reader.ident_hdr.audio_sample_rate;
    if rate == 0 {
        return Err("Invalid sample rate (0)".into());
    }
    Ok(f64::from(rate))
}

fn ogg_sample_rate_hz_ident_packet(data: &[u8]) -> Option<f64> {
    const PAGE_HEADER: usize = 27;
    const MIN_RATE_HZ: u32 = 8_000;
    const MAX_RATE_HZ: u32 = 384_000;

    let mut pos = 0usize;
    let mut packet = Vec::with_capacity(64);

    while pos + PAGE_HEADER <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            pos += 1;
            continue;
        }

        let seg_count = data[pos + 26] as usize;
        let header_end = pos.checked_add(PAGE_HEADER + seg_count)?;
        if header_end > data.len() {
            return None;
        }

        let seg_table = &data[pos + PAGE_HEADER..header_end];
        let mut body_pos = header_end;

        for &seg_len_u8 in seg_table {
            let seg_len = seg_len_u8 as usize;
            let seg_end = body_pos.checked_add(seg_len)?;
            if seg_end > data.len() {
                return None;
            }
            packet.extend_from_slice(&data[body_pos..seg_end]);
            body_pos = seg_end;

            if seg_len < 255 {
                if packet.len() < 16 || packet[0] != 0x01 || &packet[1..7] != b"vorbis" {
                    return None;
                }
                let rate = u32::from_le_bytes(packet[12..16].try_into().ok()?);
                if !(MIN_RATE_HZ..=MAX_RATE_HZ).contains(&rate) {
                    return None;
                }
                return Some(f64::from(rate));
            }
        }

        pos = body_pos;
    }

    None
}

fn find_last_granule_backwards(data: &[u8]) -> Result<u64, String> {
    const PAGE_HEADER: usize = 27;
    const CHUNK: usize = 64 * 1024;

    let mut pos = data.len();
    let mut best_granule: Option<u64> = None;

    while pos > PAGE_HEADER {
        let start = pos.saturating_sub(CHUNK);
        let chunk = &data[start..pos];

        let mut i = chunk.len().saturating_sub(PAGE_HEADER);
        while i > 0 {
            if &chunk[i..i + 4] == b"OggS" {
                let granule = u64::from_le_bytes(
                    chunk[i + 6..i + 14]
                        .try_into()
                        .map_err(|_| "Failed to read granule position".to_string())?,
                );

                if granule != u64::MAX && best_granule.is_none_or(|prev| granule > prev) {
                    best_granule = Some(granule);
                }

                // Jump back far enough to definitely get past this page.
                i = i.saturating_sub(27 + 255 * 255);
            } else {
                i -= 1;
            }
        }

        if best_granule.is_some() {
            // In almost all real-world files, the final granule is on the last page.
            break;
        }
        pos = start;
    }

    best_granule.ok_or_else(|| "No valid granule position found".into())
}
