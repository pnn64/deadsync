use super::{
    CachedChartPayload, CachedChartPayloadIndex, CachedSong, SONG_ANALYSIS_MONO_THRESHOLD,
    SONG_CACHE_MAGIC, SONG_CACHE_VERSION, SerializableSongBackgroundChangeTarget,
    SerializableSongData, build_cached_song_meta, build_gameplay_chart_from_payload,
    build_song_meta_from_cache,
};
use crate::config::dirs;
use crate::game::{chart::GameplayChartData, song::SongData};
use log::{debug, warn};
use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use twox_hash::XxHash64;

pub(super) fn compute_song_cache_path(path: &Path) -> Option<PathBuf> {
    match get_cache_path(path) {
        Ok(path) => Some(path),
        Err(error) => {
            warn!(
                "Could not generate cache path for {path:?}: {error}. Caching disabled for this file."
            );
            None
        }
    }
}

pub(super) fn load_song_from_cache(path: &Path, cache_path: &Path) -> Option<SongData> {
    let cached_song = load_cached_song(path, cache_path)?;
    Some(build_song_meta_from_cache(cached_song.data))
}

pub(super) fn write_song_cache(
    cache_path: &Path,
    source_hash: u64,
    data: &SerializableSongData,
    global_offset_seconds: f32,
) {
    let payloads = data
        .charts
        .iter()
        .map(|chart| CachedChartPayload {
            notes: chart.notes.clone(),
            parsed_notes: chart.parsed_notes.clone(),
            row_to_beat: chart.row_to_beat.clone(),
            timing_segments: chart.timing_segments.clone(),
            chart_attacks: chart.chart_attacks.clone(),
        })
        .collect::<Vec<_>>();
    let meta = build_cached_song_meta(data, global_offset_seconds);
    let mut encoded_payloads = Vec::with_capacity(payloads.len());
    let mut chart_payloads = Vec::with_capacity(payloads.len());
    let mut payload_offset = 0u64;
    for payload in payloads {
        let Ok(encoded) = bincode::encode_to_vec(&payload, bincode::config::standard()) else {
            return;
        };
        let len = encoded.len() as u64;
        chart_payloads.push(CachedChartPayloadIndex {
            offset: payload_offset,
            len,
        });
        payload_offset = payload_offset.saturating_add(len);
        encoded_payloads.push(encoded);
    }
    let cached_song = CachedSong {
        cache_version: SONG_CACHE_VERSION,
        rssp_version: rssp::RSSP_VERSION.to_string(),
        mono_threshold: SONG_ANALYSIS_MONO_THRESHOLD,
        source_hash,
        data: meta,
        chart_payloads,
    };

    let Ok(encoded_header) = bincode::encode_to_vec(&cached_song, bincode::config::standard())
    else {
        return;
    };
    let mut can_write = true;
    if let Some(parent) = cache_path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        warn!("Failed to create song cache dir {parent:?}: {error}");
        can_write = false;
    }
    if !can_write {
        return;
    }
    if let Ok(mut file) = fs::File::create(cache_path) {
        if file.write_all(&SONG_CACHE_MAGIC).is_err()
            || file
                .write_all(&(encoded_header.len() as u64).to_le_bytes())
                .is_err()
            || file.write_all(&encoded_header).is_err()
        {
            warn!("Failed to write cache file for {cache_path:?}");
            return;
        }
        for payload in encoded_payloads {
            if file.write_all(&payload).is_err() {
                warn!("Failed to write cache file for {cache_path:?}");
                return;
            }
        }
    } else {
        warn!("Failed to create cache file for {cache_path:?}");
    }
}

pub(super) fn load_gameplay_charts_from_cache(
    song: &SongData,
    requested_chart_ixs: &[usize],
    global_offset_seconds: f32,
) -> Option<Vec<GameplayChartData>> {
    let cache_path = compute_song_cache_path(&song.simfile_path)?;
    let (cached_song, payload_start) =
        load_cached_song_for_gameplay(&song.simfile_path, &cache_path)?;
    let song_offset = cached_song.data.offset;
    let mut charts = Vec::with_capacity(requested_chart_ixs.len());
    let mut loaded = HashMap::<usize, GameplayChartData>::with_capacity(requested_chart_ixs.len());
    for &chart_ix in requested_chart_ixs {
        if let Some(chart) = loaded.get(&chart_ix) {
            charts.push(chart.clone());
            continue;
        }
        let entry = *cached_song.chart_payloads.get(chart_ix)?;
        let payload = load_cached_chart_payload(&cache_path, payload_start, entry)?;
        let chart = build_gameplay_chart_from_payload(payload, song_offset, global_offset_seconds);
        loaded.insert(chart_ix, chart.clone());
        charts.push(chart);
    }
    Some(charts)
}

fn get_content_hash(path: &Path) -> Result<u64, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = XxHash64::with_seed(0);
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

    let cache_dir = dirs::app_dirs().song_cache_dir();
    let hash_hex = format!("{path_hash:016x}");
    let shard2 = &hash_hex[..2];
    Ok(cache_dir.join(shard2).join(format!("{hash_hex}.bin")))
}

#[inline(always)]
fn cached_path_exists(path_opt: Option<&str>) -> bool {
    match path_opt.map(str::trim) {
        None => true,
        Some("") => false,
        Some(path) => Path::new(path).is_file(),
    }
}

#[inline(always)]
fn cached_song_paths_exist(song: &CachedSong) -> bool {
    let data = &song.data;
    let bgchange_paths_ok = data
        .background_changes
        .iter()
        .all(|change| match &change.target {
            SerializableSongBackgroundChangeTarget::File(path) => cached_path_exists(Some(path)),
            SerializableSongBackgroundChangeTarget::NoSongBg
            | SerializableSongBackgroundChangeTarget::Random => true,
        });
    let foreground_lua_paths_ok = data
        .foreground_lua_changes
        .iter()
        .all(|change| cached_path_exists(Some(&change.path)));
    cached_path_exists(data.banner_path.as_deref())
        && cached_path_exists(data.background_path.as_deref())
        && bgchange_paths_ok
        && foreground_lua_paths_ok
        && cached_path_exists(data.cdtitle_path.as_deref())
        && cached_path_exists(data.music_path.as_deref())
}

fn load_cached_song_base(path: &Path, cache_path: &Path) -> Option<(CachedSong, u64)> {
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
        debug!(
            "Cache stale (file format mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
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
        debug!(
            "Cache stale (decode/schema mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    };

    if cached_song.cache_version != SONG_CACHE_VERSION {
        debug!(
            "Cache stale (cache version mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }
    if cached_song.rssp_version != rssp::RSSP_VERSION {
        debug!(
            "Cache stale (rssp version mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }
    if cached_song.mono_threshold != SONG_ANALYSIS_MONO_THRESHOLD {
        debug!(
            "Cache stale (mono threshold mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }
    if !cached_song_paths_exist(&cached_song) {
        debug!(
            "Cache stale (resolved asset path missing) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }
    Some((cached_song, 16 + header_len))
}

fn load_cached_song(path: &Path, cache_path: &Path) -> Option<CachedSong> {
    let (cached_song, _) = load_cached_song_base(path, cache_path)?;
    let content_hash = match get_content_hash(path) {
        Ok(hash) => hash,
        Err(error) => {
            warn!(
                "Could not hash content of {:?}: {}. Ignoring cache.",
                path.file_name().unwrap_or_default(),
                error
            );
            return None;
        }
    };

    if cached_song.source_hash != content_hash {
        debug!(
            "Cache stale (content hash mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }

    debug!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
    Some(cached_song)
}

fn load_cached_song_for_gameplay(path: &Path, cache_path: &Path) -> Option<(CachedSong, u64)> {
    let (cached_song, payload_start) = load_cached_song_base(path, cache_path)?;
    debug!(
        "Gameplay cache hit (no source rehash) for: {:?}",
        path.file_name().unwrap_or_default()
    );
    Some((cached_song, payload_start))
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
