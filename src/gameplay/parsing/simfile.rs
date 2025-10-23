use crate::gameplay::{
    chart::ChartData,
    song::{set_song_cache, SongData, SongPack},
};
use log::{info, trace, warn};
use rssp::{analyze, AnalysisOptions};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncReadExt;

use bincode::{Decode, Encode};
use futures::{future::BoxFuture, stream, StreamExt};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::hash::Hasher;
use twox_hash::XxHash64;

// --- SERIALIZABLE MIRROR STRUCTS (Unchanged) ---

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

#[derive(Serialize, Deserialize, Clone, Encode, Decode)]
struct SerializableChartData {
    chart_type: String,
    difficulty: String,
    meter: u32,
    step_artist: String,
    notes: Vec<u8>,
    short_hash: String,
    stats: CachedArrowStats,
    tech_counts: CachedTechCounts,
    total_streams: u32,
    max_nps: f64,
    detailed_breakdown: String,
    partial_breakdown: String,
    simple_breakdown: String,
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
            short_hash: chart.short_hash.clone(),
            stats: (&chart.stats).into(),
            tech_counts: (&chart.tech_counts).into(),
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
            detailed_breakdown: chart.detailed_breakdown.clone(),
            partial_breakdown: chart.partial_breakdown.clone(),
            simple_breakdown: chart.simple_breakdown.clone(),
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
            short_hash: chart.short_hash,
            stats: chart.stats.into(),
            tech_counts: chart.tech_counts.into(),
            total_streams: chart.total_streams,
            max_nps: chart.max_nps,
            detailed_breakdown: chart.detailed_breakdown,
            partial_breakdown: chart.partial_breakdown,
            simple_breakdown: chart.simple_breakdown,
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
    offset: f32,
    sample_start: Option<f32>,
    sample_length: Option<f32>,
    min_bpm: f64,
    max_bpm: f64,
    normalized_bpms: String,
    total_length_seconds: i32,
    charts: Vec<SerializableChartData>,
}

impl From<&SongData> for SerializableSongData {
    fn from(song: &SongData) -> Self {
        Self {
            title: song.title.clone(),
            subtitle: song.subtitle.clone(),
            artist: song.artist.clone(),
            banner_path: song.banner_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            background_path: song.background_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            music_path: song.music_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            offset: song.offset,
            sample_start: song.sample_start,
            sample_length: song.sample_length,
            min_bpm: song.min_bpm,
            max_bpm: song.max_bpm,
            normalized_bpms: song.normalized_bpms.clone(),
            total_length_seconds: song.total_length_seconds,
            charts: song.charts.iter().map(SerializableChartData::from).collect(),
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
            offset: song.offset,
            sample_start: song.sample_start,
            sample_length: song.sample_length,
            min_bpm: song.min_bpm,
            max_bpm: song.max_bpm,
            normalized_bpms: song.normalized_bpms,
            total_length_seconds: song.total_length_seconds,
            charts: song.charts.into_iter().map(ChartData::from).collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Encode, Decode)]
struct CachedSong {
    source_hash: u64,
    data: SerializableSongData,
}

// --- ASYNC HELPERS ---

async fn get_content_hash(path: &Path) -> Result<u64, std::io::Error> {
    let mut file = fs::File::open(path).await?;
    let mut hasher = XxHash64::with_seed(0);
    let mut buffer = [0; 8192];
    loop {
        let bytes_read = file.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        hasher.write(&buffer[..bytes_read]);
    }
    Ok(hasher.finish())
}

fn get_cache_path(simfile_path: &Path) -> Result<PathBuf, std::io::Error> {
    let canonical_path = std::fs::canonicalize(simfile_path)?;
    let mut hasher = XxHash64::with_seed(0);
    hasher.write(canonical_path.to_string_lossy().as_bytes());
    let path_hash = hasher.finish();

    let cache_dir = Path::new("cache/songs");
    let file_name = format!("{:x}.bin", path_hash);
    Ok(cache_dir.join(file_name))
}

fn find_simfiles_recursive<'a>(
    dir: PathBuf,
    simfiles: &'a mut Vec<PathBuf>,
) -> BoxFuture<'a, ()> {
    Box::pin(async move {
        let mut read_dir = match fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(_) => return,
        };

        while let Some(entry) = read_dir.next_entry().await.ok().flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_simfiles_recursive(path, simfiles).await;
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("sm") || ext.eq_ignore_ascii_case("ssc") {
                    simfiles.push(path);
                }
            }
        }
    })
}

/// The result of the asynchronous I/O phase for a single simfile.
enum LoadAttempt {
    Cached(SongData),
    NeedsParsing {
        path: PathBuf,
        content: Vec<u8>,
        content_hash: u64,
    },
    Error(String),
}

/// Phase 1: Asynchronously check cache or read file content for parsing.
async fn load_or_prepare_parse(path: PathBuf, fastload: bool) -> LoadAttempt {
    if !fastload {
        return match fs::read(&path).await {
            Ok(content) => {
                let content_hash = get_content_hash(&path).await.unwrap_or(0);
                LoadAttempt::NeedsParsing { path, content, content_hash }
            }
            Err(e) => LoadAttempt::Error(format!("Failed to read file {:?}: {}", path, e)),
        };
    }

    let cache_path_res = get_cache_path(&path);
    let content_hash_res = get_content_hash(&path).await;

    if let (Ok(cp), Ok(ch)) = (&cache_path_res, &content_hash_res) {
        if let Ok(cache_bytes) = fs::read(cp).await {
            // Decoding can be slow, move to a blocking thread.
            let cached_song_res = tokio::task::spawn_blocking(move || {
                bincode::decode_from_slice::<CachedSong, _>(&cache_bytes, bincode::config::standard())
            }).await;

            if let Ok(Ok((cached_song, _))) = cached_song_res {
                if cached_song.source_hash == *ch {
                    return LoadAttempt::Cached(cached_song.data.into());
                }
            }
        }
    }

    // Cache miss or error, read the file for parsing.
    match fs::read(&path).await {
        Ok(content) => {
            let content_hash = content_hash_res.unwrap_or(0);
            LoadAttempt::NeedsParsing { path, content, content_hash }
        }
        Err(e) => LoadAttempt::Error(format!("Failed to read file {:?}: {}", path, e)),
    }
}

pub async fn scan_and_load_songs(root_path_str: &'static str) {
    info!("Starting async simfile scan in '{}'...", root_path_str);
    let start_time = std::time::Instant::now();
    let config = crate::config::get();

    // Setup cache dir
    let cache_dir = Path::new("cache/songs");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        warn!("Could not create cache directory '{}': {}. Caching will be disabled.", cache_dir.to_string_lossy(), e);
    }

    // Find all simfiles
    let root_path = Path::new(root_path_str);
    if !root_path.exists() {
        warn!("Songs directory '{}' not found. No songs will be loaded.", root_path_str);
        return;
    }
    let mut all_simfile_paths = Vec::new();
    find_simfiles_recursive(root_path.to_path_buf(), &mut all_simfile_paths).await;
    info!("Found {} potential simfiles. Starting concurrent I/O phase...", all_simfile_paths.len());

    // --- Phase 1: Concurrent I/O (Cache check / File read) ---
    let io_results: Vec<LoadAttempt> = stream::iter(all_simfile_paths)
        .map(|path| load_or_prepare_parse(path, config.fastload))
        .buffer_unordered(256) // High concurrency for I/O
        .collect()
        .await;

    let mut songs_from_cache: Vec<Arc<SongData>> = Vec::new();
    let mut to_be_parsed = Vec::new();
    for result in io_results {
        match result {
            LoadAttempt::Cached(song) => songs_from_cache.push(Arc::new(song)),
            LoadAttempt::NeedsParsing { path, content, content_hash } => to_be_parsed.push((path, content, content_hash)),
            LoadAttempt::Error(e) => warn!("{}", e),
        }
    }
    info!("I/O phase complete. {} songs loaded from cache, {} need parsing.", songs_from_cache.len(), to_be_parsed.len());

    // --- Phase 2: Parallel CPU Processing (Parsing & Caching) ---
    let newly_parsed_songs = if !to_be_parsed.is_empty() {
        tokio::task::spawn_blocking(move || {
            to_be_parsed
                .par_iter()
                .filter_map(|(path, content, content_hash)| {
                    match parse_and_process_song_file_sync(path, content) {
                        Ok(song_data) => {
                            if config.cachesongs {
                                if let Ok(cp) = get_cache_path(path) {
                                    let serializable_data: SerializableSongData = (&song_data).into();
                                    let cached_song = CachedSong { source_hash: *content_hash, data: serializable_data };
                                    if let Ok(encoded) = bincode::encode_to_vec(&cached_song, bincode::config::standard()) {
                                        if let Err(e) = std::fs::write(cp, &encoded) {
                                            warn!("Failed to write cache file: {}", e);
                                        }
                                    }
                                }
                            }
                            Some(song_data)
                        }
                        Err(e) => {
                            warn!("Failed to parse '{:?}': {}", path, e);
                            None
                        }
                    }
                })
                .collect::<Vec<SongData>>()
        })
        .await
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    info!("CPU phase complete. {} new songs parsed.", newly_parsed_songs.len());

    // --- Phase 3: Combine & Finalize ---
    let mut all_loaded_songs: Vec<Arc<SongData>> = songs_from_cache;
    all_loaded_songs.extend(newly_parsed_songs.into_iter().map(Arc::new));

    let mut packs_map: HashMap<PathBuf, Vec<Arc<SongData>>> = HashMap::new();
    for song in all_loaded_songs {
        if let Some(music_path) = &song.music_path {
            if let Some(song_dir) = music_path.parent() {
                if let Some(pack_dir) = song_dir.parent() {
                    packs_map.entry(pack_dir.to_path_buf()).or_default().push(song.clone());
                }
            }
        }
    }

    let mut loaded_packs: Vec<SongPack> = packs_map
        .into_iter()
        .map(|(pack_path, mut songs)| {
            songs.sort_by(|a, b| {
                let a_title = a.title.to_lowercase();
                let b_title = b.title.to_lowercase();
                let a_is_special = a_title.chars().next().map_or(false, |c| !c.is_alphanumeric());
                let b_is_special = b_title.chars().next().map_or(false, |c| !c.is_alphanumeric());

                if a_is_special == b_is_special { a_title.cmp(&b_title) } 
                else if a_is_special { std::cmp::Ordering::Greater } 
                else { std::cmp::Ordering::Less }
            });
            SongPack {
                name: pack_path.file_name().unwrap_or_default().to_string_lossy().to_string(),
                songs,
            }
        })
        .collect();
    
    loaded_packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    info!(
        "Finished full scan in {:.2}s. Loaded {} packs.",
        start_time.elapsed().as_secs_f32(),
        loaded_packs.len()
    );
    set_song_cache(loaded_packs);
}

/// Synchronous version of the parsing logic, designed to be run in `spawn_blocking`.
fn parse_and_process_song_file_sync(path: &Path, simfile_data: &[u8]) -> Result<SongData, String> {
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let options = AnalysisOptions::default();

    let summary = analyze(simfile_data, extension, options)?;

    let charts: Vec<ChartData> = summary
        .charts
        .into_iter()
        .map(|c| {
            trace!(
                "  Chart '{}' [{}] loaded with {} bytes of note data.",
                c.difficulty_str, c.rating_str, c.minimized_note_data.len()
            );
            ChartData {
                chart_type: c.step_type_str,
                difficulty: c.difficulty_str,
                meter: c.rating_str.parse().unwrap_or(0),
                step_artist: c.step_artist_str.join(", "),
                notes: c.minimized_note_data,
                short_hash: c.short_hash,
                stats: c.stats,
                tech_counts: c.tech_counts,
                total_streams: c.total_streams,
                total_measures: c.total_measures,
                max_nps: c.max_nps,
                detailed_breakdown: c.detailed,
                partial_breakdown: c.partial,
                simple_breakdown: c.simple,
                measure_nps_vec: c.measure_nps_vec,
            }
        })
        .collect();

    let simfile_dir = path.parent().ok_or_else(|| "Could not determine simfile directory".to_string())?;

    let mut background_path_opt: Option<PathBuf> = if !summary.background_path.is_empty() {
        let p = simfile_dir.join(&summary.background_path);
        if p.exists() { Some(p) } else { None }
    } else {
        None
    };

    if background_path_opt.is_none() {
        if let Ok(entries) = std::fs::read_dir(simfile_dir) {
            let image_files: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file() &&
                    p.extension().and_then(|s| s.to_str()).map_or(false, |ext| {
                        matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "bmp")
                    })
                })
                .collect();
            
            let mut found_bg: Option<String> = None;
            for file in &image_files {
                if let Some(file_name) = file.file_name().and_then(|s| s.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    if file_name_lower.contains("background") || file_name_lower.contains("bg") {
                        found_bg = Some(file_name.to_string());
                        break;
                    }
                }
            }

            if found_bg.is_none() {
                for file in &image_files {
                    if let Some(file_name) = file.file_name().and_then(|s| s.to_str()) {
                         if let Ok((w, h)) = image::image_dimensions(file) {
                             if w >= 320 && h >= 240 {
                                let aspect = if h > 0 { w as f32 / h as f32 } else { 0.0 };
                                if aspect < 2.0 {
                                    found_bg = Some(file_name.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            
            if let Some(bg_filename) = found_bg {
                background_path_opt = Some(simfile_dir.join(bg_filename));
            }
        }
    }

    let banner_path = if !summary.banner_path.is_empty() {
        let p = simfile_dir.join(&summary.banner_path);
        if p.exists() { Some(p) } else { None }
    } else {
        None
    };

    let music_path = if !summary.music_path.is_empty() {
        Some(simfile_dir.join(summary.music_path))
    } else {
        None
    };

    Ok(SongData {
        title: summary.title_str,
        subtitle: summary.subtitle_str,
        artist: summary.artist_str,
        banner_path,
        background_path: background_path_opt,
        offset: summary.offset as f32,
        sample_start: if summary.sample_start > 0.0 { Some(summary.sample_start as f32) } else { None },
        sample_length: if summary.sample_length > 0.0 { Some(summary.sample_length as f32) } else { None },
        min_bpm: summary.min_bpm,
        max_bpm: summary.max_bpm,
        normalized_bpms: summary.normalized_bpms,
        music_path,
        total_length_seconds: summary.total_length,
        charts,
    })
}
