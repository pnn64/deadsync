use crate::game::{
    chart::ChartData,
    note::NoteType,
    parsing::notes::ParsedNote,
    song::{SongData, SongPack, set_song_cache},
    timing::{
        DelaySegment, FakeSegment, ScrollSegment, SpeedSegment, SpeedUnit, StopSegment, TimingData,
        TimingSegments, WarpSegment,
    },
};
use log::{info, warn};
use rssp::{AnalysisOptions, analyze};
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
enum CachedSpeedUnit {
    Beats,
    Seconds,
}

impl From<SpeedUnit> for CachedSpeedUnit {
    fn from(unit: SpeedUnit) -> Self {
        match unit {
            SpeedUnit::Beats => CachedSpeedUnit::Beats,
            SpeedUnit::Seconds => CachedSpeedUnit::Seconds,
        }
    }
}

impl From<CachedSpeedUnit> for SpeedUnit {
    fn from(unit: CachedSpeedUnit) -> Self {
        match unit {
            CachedSpeedUnit::Beats => SpeedUnit::Beats,
            CachedSpeedUnit::Seconds => SpeedUnit::Seconds,
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
            NoteType::Tap => CachedNoteType::Tap,
            NoteType::Hold => CachedNoteType::Hold,
            NoteType::Roll => CachedNoteType::Roll,
            NoteType::Mine => CachedNoteType::Mine,
            NoteType::Fake => CachedNoteType::Fake,
        }
    }
}

impl From<CachedNoteType> for NoteType {
    fn from(note_type: CachedNoteType) -> Self {
        match note_type {
            CachedNoteType::Tap => NoteType::Tap,
            CachedNoteType::Hold => NoteType::Hold,
            CachedNoteType::Roll => NoteType::Roll,
            CachedNoteType::Mine => NoteType::Mine,
            CachedNoteType::Fake => NoteType::Fake,
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
    total_streams: u32,
    max_nps: f64,
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
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
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
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
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
    title: String,
    subtitle: String,
    artist: String,
    banner_path: Option<String>,
    background_path: Option<String>,
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
            title: song.title.clone(),
            subtitle: song.subtitle.clone(),
            artist: song.artist.clone(),
            banner_path: song
                .banner_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            background_path: song
                .background_path
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
            title: song.title,
            subtitle: song.subtitle,
            artist: song.artist,
            banner_path: song.banner_path.map(PathBuf::from),
            background_path: song.background_path.map(PathBuf::from),
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
    source_hash: u64,
    data: SerializableSongData,
}

// --- CACHING HELPER FUNCTIONS ---

#[derive(Clone)]
struct SongCacheKeys {
    cache_path: Option<PathBuf>,
    content_hash: Option<u64>,
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
    let file_name = format!("{:x}.bin", path_hash);
    Ok(cache_dir.join(file_name))
}

fn compute_song_cache_keys(path: &Path) -> SongCacheKeys {
    let cache_path = match get_cache_path(path) {
        Ok(p) => Some(p),
        Err(e) => {
            warn!(
                "Could not generate cache path for {:?}: {}. Caching disabled for this file.",
                path, e
            );
            None
        }
    };
    let content_hash = match get_content_hash(path) {
        Ok(h) => Some(h),
        Err(e) => {
            warn!(
                "Could not hash content of {:?}: {}. Caching disabled for this file.",
                path, e
            );
            None
        }
    };
    SongCacheKeys {
        cache_path,
        content_hash,
    }
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
    let s = total_s - (m as f64 * 60.0);
    format!("{m}m{s:.1}s")
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

/// Scans the provided root directory (e.g., "songs/") for simfiles,
/// parses them, and populates the global cache. This should be run once at startup.
pub fn scan_and_load_songs(root_path_str: &'static str) {
    info!("Starting simfile scan in '{}'...", root_path_str);

    let started = Instant::now();
    let config = crate::config::get();
    let fastload = config.fastload;
    let cachesongs = config.cachesongs;
    let global_offset_seconds = config.global_offset_seconds;

    let avail_threads = std::thread::available_parallelism()
        .map(|n| n.get())
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
        warn!(
            "Songs directory '{}' not found. No songs will be loaded.",
            root_path_str
        );
        return;
    }

    let mut loaded_packs = Vec::new();
    let mut songs_cache_hits = 0usize;
    let mut songs_parsed = 0usize;
    let mut songs_failed = 0usize;

    let packs = match rssp::pack::scan_songs_dir(root_path, rssp::pack::ScanOpt::default()) {
        Ok(p) => p,
        Err(e) => {
            warn!("Could not scan songs dir '{}': {:?}", root_path_str, e);
            return;
        }
    };

    type ParseMsg = (usize, PathBuf, Result<Arc<SongData>, String>);

    let mut runtime: Option<tokio::runtime::Runtime> = None;
    let mut tx_opt: Option<std::sync::mpsc::Sender<ParseMsg>> = None;
    let mut rx_opt: Option<std::sync::mpsc::Receiver<ParseMsg>> = None;
    let mut in_flight = 0usize;

    fn reap_one(
        rx: Option<&std::sync::mpsc::Receiver<ParseMsg>>,
        in_flight: &mut usize,
        loaded_packs: &mut Vec<SongPack>,
        songs_failed: &mut usize,
    ) {
        let Some(rx) = rx else {
            return;
        };
        match rx.recv() {
            Ok((pack_idx, simfile_path, result)) => {
                *in_flight = in_flight.saturating_sub(1);
                match result {
                    Ok(song_data) => {
                        if let Some(pack) = loaded_packs.get_mut(pack_idx) {
                            pack.songs.push(song_data);
                        }
                    }
                    Err(e) => {
                        *songs_failed += 1;
                        warn!("Failed to load '{:?}': {}", simfile_path, e)
                    }
                }
            }
            Err(_) => {
                *in_flight = 0;
            }
        }
    }

    for pack in packs {
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
            let cache_keys = if fastload || cachesongs {
                compute_song_cache_keys(&simfile_path)
            } else {
                SongCacheKeys {
                    cache_path: None,
                    content_hash: None,
                }
            };

            if fastload
                && let (Some(cp), Some(ch)) = (&cache_keys.cache_path, cache_keys.content_hash)
                && let Some(song_data) =
                    load_song_from_cache(&simfile_path, cp, ch, global_offset_seconds)
            {
                songs_cache_hits += 1;
                loaded_packs[pack_idx].songs.push(Arc::new(song_data));
                continue;
            }

            songs_parsed += 1;
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
                    );
                }

                let Some(tx) = tx_opt.as_ref() else {
                    warn!("Song parsing worker channel unavailable; falling back to sync parse.");
                    match parse_song_and_maybe_write_cache(
                        &simfile_path,
                        fastload,
                        cachesongs,
                        cache_keys,
                        global_offset_seconds,
                    ) {
                        Ok(song_data) => loaded_packs[pack_idx].songs.push(Arc::new(song_data)),
                        Err(e) => {
                            songs_failed += 1;
                            warn!("Failed to load '{:?}': {}", simfile_path, e)
                        }
                    }
                    continue;
                };

                let tx = tx.clone();
                let simfile_path_owned = simfile_path.clone();
                rt.handle().spawn_blocking(move || {
                    let out = catch_unwind(AssertUnwindSafe(|| {
                        parse_song_and_maybe_write_cache(
                            &simfile_path,
                            fastload,
                            cachesongs,
                            cache_keys,
                            global_offset_seconds,
                        )
                        .map(Arc::new)
                    }))
                    .unwrap_or_else(|_| Err("Song parse panicked".to_string()));
                    let _ = tx.send((pack_idx, simfile_path_owned, out));
                });
                in_flight += 1;
                continue;
            }

            match parse_song_and_maybe_write_cache(
                &simfile_path,
                fastload,
                cachesongs,
                cache_keys,
                global_offset_seconds,
            ) {
                Ok(song_data) => loaded_packs[pack_idx].songs.push(Arc::new(song_data)),
                Err(e) => {
                    songs_failed += 1;
                    warn!("Failed to load '{:?}': {}", simfile_path, e)
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
        );
    }

    if runtime.is_some() {
        info!(
            "Song parsing: used {} threads for cache misses (SongParsingThreads={}).",
            parse_threads, config.song_parsing_threads
        );
    }

    loaded_packs.retain(|p| !p.songs.is_empty());
    for pack in &mut loaded_packs {
        pack.songs.sort_by(|a, b| {
            let a_title = a.title.to_lowercase();
            let b_title = b.title.to_lowercase();

            let a_first_char = a_title.chars().next();
            let b_first_char = b_title.chars().next();

            // Treat a title as "special" if it starts with a non-alphanumeric character.
            let a_is_special = a_first_char.is_some_and(|c| !c.is_alphanumeric());
            let b_is_special = b_first_char.is_some_and(|c| !c.is_alphanumeric());

            if a_is_special == b_is_special {
                // If both are special or both are not, sort them alphabetically.
                a_title.cmp(&b_title)
            } else if a_is_special {
                // `a` is special and `b` is not, so `b` should come first.
                std::cmp::Ordering::Greater
            } else {
                // `b` is special and `a` is not, so `a` should come first.
                std::cmp::Ordering::Less
            }
        });
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

fn load_song_from_cache(
    path: &Path,
    cache_path: &Path,
    content_hash: u64,
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

    if cached_song.source_hash == content_hash && cached_song.rssp_version == rssp::RSSP_VERSION {
        info!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
        let mut song_data: SongData = cached_song.data.into();
        hydrate_chart_timings(&mut song_data, global_offset_seconds);
        return Some(song_data);
    }
    if cached_song.source_hash != content_hash {
        info!(
            "Cache stale (content hash mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
    } else {
        info!(
            "Cache stale (rssp version mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    None
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
    let song_data = parse_and_process_song_file(path)?;

    if cachesongs && let (Some(cp), Some(ch)) = (cache_keys.cache_path, cache_keys.content_hash) {
        let serializable_data: SerializableSongData = (&song_data).into();
        let cached_song = CachedSong {
            rssp_version: rssp::RSSP_VERSION.to_string(),
            source_hash: ch,
            data: serializable_data,
        };

        if let Ok(encoded) = bincode::encode_to_vec(&cached_song, bincode::config::standard()) {
            if let Ok(mut file) = fs::File::create(&cp) {
                if file.write_all(&encoded).is_err() {
                    warn!("Failed to write cache file for {:?}", cp);
                }
            } else {
                warn!("Failed to create cache file for {:?}", cp);
            }
        }
    }

    let mut song_data = song_data;
    hydrate_chart_timings(&mut song_data, global_offset_seconds);
    Ok(song_data)
}

/// The original parsing logic, now separated to be called on a cache miss.
fn parse_and_process_song_file(path: &Path) -> Result<SongData, String> {
    let simfile_data = fs::read(path).map_err(|e| format!("Could not read file: {}", e))?;
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let options = AnalysisOptions::default(); // Use default parsing options

    let summary = analyze(&simfile_data, extension, options)?;
    let charts: Vec<ChartData> = summary
        .charts
        .into_iter()
        .map(|c| {
            let lanes = step_type_lanes(&c.step_type_str);
            let parsed_notes =
                crate::game::parsing::notes::parse_chart_notes(&c.minimized_note_data, lanes);
            info!(
                "  Chart '{}' [{}] loaded with {} bytes of note data.",
                c.difficulty_str,
                c.rating_str,
                c.minimized_note_data.len()
            );
            ChartData {
                chart_type: c.step_type_str,
                difficulty: c.difficulty_str,
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
                total_streams: c.total_streams,
                total_measures: c.total_measures,
                max_nps: c.max_nps,
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

    let music_path = if !summary.music_path.is_empty() {
        Some(simfile_dir.join(summary.music_path))
    } else {
        None
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

    Ok(SongData {
        title: summary.title_str,
        subtitle: summary.subtitle_str,
        artist: summary.artist_str,
        banner_path, // Keep original logic for banner
        background_path: background_path_opt,
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
    })
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
        .map(|ext| {
            let ext_lower = ext.to_ascii_lowercase();
            ext_lower == "ogg" || ext_lower == "oga"
        })
        .unwrap_or(false);
    if !ext_is_ogg {
        return 0.0;
    }

    match ogg_length_seconds(path) {
        Ok(sec) => sec,
        Err(e) => {
            warn!("Failed to compute OGG length for {:?}: {}", path, e);
            0.0
        }
    }
}

/// Fast OGG length detection: use lewton only for headers (sample rate) and
/// scan backwards through the file to find the last valid granule position.
fn ogg_length_seconds(path: &Path) -> Result<f32, String> {
    let file = fs::File::open(path).map_err(|e| format!("Cannot open file: {}", e))?;

    // Safe wrapper around the unsafe memmap2 API.
    let mmap = unsafe { Mmap::map(&file) }.map_err(|e| format!("Memory-map failed: {}", e))?;

    // Use lewton to get the sample rate from the header.
    let sample_rate_hz = {
        let cursor = Cursor::new(&mmap[..]);
        let reader = OggStreamReader::new(BufReader::new(cursor))
            .map_err(|e| format!("lewton header error: {}", e))?;
        let rate = reader.ident_hdr.audio_sample_rate;
        if rate == 0 {
            return Err("Invalid sample rate (0)".into());
        }
        rate as f64
    };

    let total_samples = find_last_granule_backwards(&mmap)?;
    Ok((total_samples as f64 / sample_rate_hz) as f32)
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
