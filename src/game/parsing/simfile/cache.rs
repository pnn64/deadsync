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
use std::time::UNIX_EPOCH;
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

pub(super) fn load_song_from_cache(
    path: &Path,
    cache_path: &Path,
    verify_freshness: bool,
) -> Option<SongData> {
    let cached_song = load_cached_song(path, cache_path, verify_freshness)?;
    Some(build_song_meta_from_cache(cached_song.data))
}

pub(super) fn write_song_cache(
    cache_path: &Path,
    data: &SerializableSongData,
    global_offset_seconds: f32,
) {
    let directory_hash = match get_song_directory_hash(Path::new(&data.simfile_path)) {
        Ok(hash) => hash,
        Err(error) => {
            warn!(
                "Could not hash song directory for {:?}: {}. Cache write skipped.",
                Path::new(&data.simfile_path)
                    .file_name()
                    .unwrap_or_default(),
                error
            );
            return;
        }
    };
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
        directory_hash,
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
    verify_freshness: bool,
) -> Option<Vec<GameplayChartData>> {
    let cache_path = compute_song_cache_path(&song.simfile_path)?;
    let (cached_song, payload_start) =
        load_cached_song_for_gameplay(&song.simfile_path, &cache_path, verify_freshness)?;
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

fn load_cached_song(path: &Path, cache_path: &Path, verify_freshness: bool) -> Option<CachedSong> {
    let (cached_song, _) = load_cached_song_base(path, cache_path)?;
    if verify_freshness {
        validate_directory_hash(path, &cached_song)?;
    }

    debug!("Cache hit for: {:?}", path.file_name().unwrap_or_default());
    Some(cached_song)
}

fn load_cached_song_for_gameplay(
    path: &Path,
    cache_path: &Path,
    verify_freshness: bool,
) -> Option<(CachedSong, u64)> {
    let (cached_song, payload_start) = load_cached_song_base(path, cache_path)?;
    if verify_freshness {
        validate_directory_hash(path, &cached_song)?;
        debug!(
            "Gameplay cache hit for: {:?}",
            path.file_name().unwrap_or_default()
        );
    } else {
        debug!(
            "Gameplay cache hit (no freshness check) for: {:?}",
            path.file_name().unwrap_or_default()
        );
    }
    Some((cached_song, payload_start))
}

fn validate_directory_hash(path: &Path, cached_song: &CachedSong) -> Option<()> {
    let directory_hash = match get_song_directory_hash(path) {
        Ok(hash) => hash,
        Err(error) => {
            warn!(
                "Could not hash song directory for {:?}: {}. Ignoring cache.",
                path.file_name().unwrap_or_default(),
                error
            );
            return None;
        }
    };

    if cached_song.directory_hash != directory_hash {
        debug!(
            "Cache stale (directory hash mismatch) for: {:?}",
            path.file_name().unwrap_or_default()
        );
        return None;
    }

    Some(())
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
    use crate::game::parsing::simfile::SerializableSongData;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("deadsync-cache-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
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
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts: Vec::new(),
        }
    }

    #[test]
    fn gameplay_cache_rejects_stale_directory_when_verifying() {
        let root = test_dir("gameplay-stale-directory-verified");
        let simfile = root.join("song.ssc");
        let cache_path = root.join("cache.bin");
        fs::write(&simfile, b"#TITLE:Old;").unwrap();
        write_song_cache(&cache_path, &cached_song(&simfile), 0.0);

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
        write_song_cache(&cache_path, &cached_song(&simfile), 0.0);

        fs::write(root.join("banner.png"), b"new asset").unwrap();

        assert!(load_cached_song_for_gameplay(&simfile, &cache_path, false).is_some());
        let _ = fs::remove_dir_all(root);
    }
}
