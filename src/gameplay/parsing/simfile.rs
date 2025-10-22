use crate::gameplay::{
    chart::ChartData,
    song::{set_song_cache, SongData, SongPack},
};
use log::{info, warn};
use rssp::{analyze, AnalysisOptions};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use twox_hash::XxHash64;
use std::hash::Hasher;
use bincode::{Decode, Encode};

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

// --- CACHING HELPER FUNCTIONS ---

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


/// Scans the provided root directory (e.g., "songs/") for simfiles,
/// parses them, and populates the global cache. This should be run once at startup.
pub fn scan_and_load_songs(root_path_str: &'static str) {
    info!("Starting simfile scan in '{}'...", root_path_str);

    let config = crate::config::get();

    // Ensure the cache directory exists before we start scanning.
    let cache_dir = Path::new("cache/songs");
    if let Err(e) = fs::create_dir_all(cache_dir) {
        warn!("Could not create cache directory '{}': {}. Caching will be disabled.", cache_dir.to_string_lossy(), e);
    }

    let root_path = Path::new(root_path_str);
    if !root_path.exists() || !root_path.is_dir() {
        warn!("Songs directory '{}' not found. No songs will be loaded.", root_path_str);
        return;
    }

    let mut loaded_packs = Vec::new();

    // Each directory inside the root is considered a "pack"
    for pack_dir_entry in fs::read_dir(root_path).into_iter().flatten().flatten() {
        let pack_path = pack_dir_entry.path();
        if !pack_path.is_dir() {
            continue;
        }

        let pack_name = pack_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let mut current_pack = SongPack { name: pack_name, songs: Vec::new() };
        info!("Scanning pack: {}", current_pack.name);

        // Each subdirectory in a pack is a song folder
        for song_dir_entry in fs::read_dir(pack_path).into_iter().flatten().flatten() {
            let song_path = song_dir_entry.path();
            if !song_path.is_dir() {
                continue;
            }

            // Find the .sm or .ssc file within the song folder
            if let Ok(files) = fs::read_dir(&song_path) {
                for file in files.flatten() {
                    let file_path = file.path();
                    if let Some(ext) = file_path.extension().and_then(|s| s.to_str()) {
                        if ext.eq_ignore_ascii_case("sm") || ext.eq_ignore_ascii_case("ssc") {
                            match load_song_from_file(&file_path, config.fastload, config.cachesongs) {
                                Ok(song_data) => {
                                    current_pack.songs.push(Arc::new(song_data));
                                }
                                Err(e) => warn!("Failed to load '{:?}': {}", file_path, e),
                            }
                            // Found the simfile, move to the next song directory
                            break;
                        }
                    }
                }
            }
        }

        if !current_pack.songs.is_empty() {
            // Sort songs within the pack with a more natural order, grouping songs
            // that start with non-alphanumeric characters (like '[Marathon]') at the end.
            current_pack.songs.sort_by(|a, b| {
                let a_title = a.title.to_lowercase();
                let b_title = b.title.to_lowercase();

                let a_first_char = a_title.chars().next();
                let b_first_char = b_title.chars().next();

                // Treat a title as "special" if it starts with a non-alphanumeric character.
                let a_is_special = a_first_char.map_or(false, |c| !c.is_alphanumeric());
                let b_is_special = b_first_char.map_or(false, |c| !c.is_alphanumeric());

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
            loaded_packs.push(current_pack);
        }
    }

    // Sort the packs themselves alphabetically by name for consistent ordering.
    loaded_packs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    info!("Finished scan. Found {} packs.", loaded_packs.len());
    set_song_cache(loaded_packs);
}

/// Helper function to parse a single simfile, using a cache if available and valid.
fn load_song_from_file(path: &Path, fastload: bool, cachesongs: bool) -> Result<SongData, String> {
    let cache_path = match get_cache_path(path) {
        Ok(p) => Some(p),
        Err(e) => {
            warn!("Could not generate cache path for {:?}: {}. Caching disabled for this file.", path, e);
            None
        }
    };

    let content_hash = match get_content_hash(path) {
        Ok(h) => Some(h),
        Err(e) => {
            warn!("Could not hash content of {:?}: {}. Caching disabled for this file.", path, e);
            None
        }
    };

    // --- CACHE CHECK ---
    if fastload {
        if let (Some(cp), Some(ch)) = (cache_path.as_ref(), content_hash) {
            if cp.exists() {
                if let Ok(mut file) = fs::File::open(cp) {
                    let mut buffer = Vec::new();
                    if file.read_to_end(&mut buffer).is_ok() {
                        if let Ok((cached_song, _)) = bincode::decode_from_slice::<CachedSong, _>(&buffer, bincode::config::standard()) {
                            if cached_song.source_hash == ch {
                                info!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
                                return Ok(cached_song.data.into());
                            } else {
                                info!("Cache stale for: {:?}", path.file_name().unwrap_or_default());
                            }
                        }
                    }
                }
            }
        }
    }

    // --- CACHE MISS: PARSE AND WRITE ---
    if fastload {
        info!("Cache miss for: {:?}", path.file_name().unwrap_or_default());
    } else {
        info!("Parsing (fastload disabled): {:?}", path.file_name().unwrap_or_default());
    }
    let song_data = parse_and_process_song_file(path)?;

    if cachesongs {
        if let (Some(cp), Some(ch)) = (cache_path, content_hash) {
            let serializable_data: SerializableSongData = (&song_data).into();
            let cached_song = CachedSong {
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
    }

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

    // --- Background Path Logic (with autodetection) ---
    let mut background_path_opt: Option<PathBuf> = if !summary.background_path.is_empty() {
        let p = simfile_dir.join(&summary.background_path);
        if p.exists() { Some(p) } else { None }
    } else {
        None
    };

    if background_path_opt.is_none() {
        info!("'{}' - BG path is missing or empty, attempting autodetection.", summary.title_str);
        if let Ok(entries) = fs::read_dir(simfile_dir) {
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

            // Hint-based search first
            for file in &image_files {
                if let Some(file_name) = file.file_name().and_then(|s| s.to_str()) {
                    let file_name_lower = file_name.to_lowercase();
                    if file_name_lower.contains("background") || file_name_lower.contains("bg") {
                        found_bg = Some(file_name.to_string());
                        break;
                    }
                }
            }

            // Dimension-based search if no hint match
            if found_bg.is_none() {
                for file in &image_files {
                    if let Some(file_name) = file.file_name().and_then(|s| s.to_str()) {
                         if let Ok((w, h)) = image::image_dimensions(file) {
                             if w >= 320 && h >= 240 {
                                let aspect = if h > 0 { w as f32 / h as f32 } else { 0.0 };
                                if aspect < 2.0 { // Banners are usually wider than 2:1
                                    found_bg = Some(file_name.to_string());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            
            if let Some(bg_filename) = found_bg {
                info!("Autodetected background: '{}'", bg_filename);
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
        banner_path, // Keep original logic for banner
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
