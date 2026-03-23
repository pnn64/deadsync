use crate::config::SimpleIni;
use crate::core::input::InputSource;
use crate::core::network;
use crate::game::downloads;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::profile::{self, Profile};
use crate::game::song::get_song_cache;
use crate::game::stage_stats;
use chrono::{Local, TimeZone};
use log::{debug, warn};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bincode::{Decode, Encode};

// --- Grade Definitions ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
#[allow(dead_code)] // Quint will be used eventually for W0 tracking
pub enum Grade {
    Quint,
    Tier01,
    Tier02,
    Tier03,
    Tier04,
    Tier05,
    Tier06,
    Tier07,
    Tier08,
    Tier09,
    Tier10,
    Tier11,
    Tier12,
    Tier13,
    Tier14,
    Tier15,
    Tier16,
    Tier17,
    Failed,
}

impl Grade {
    /// Converts a grade to the corresponding frame index on the "grades 1x19.png" spritesheet.
    pub const fn to_sprite_state(&self) -> u32 {
        match self {
            Self::Quint => 0,
            Self::Tier01 => 1,
            Self::Tier02 => 2,
            Self::Tier03 => 3,
            Self::Tier04 => 4,
            Self::Tier05 => 5,
            Self::Tier06 => 6,
            Self::Tier07 => 7,
            Self::Tier08 => 8,
            Self::Tier09 => 9,
            Self::Tier10 => 10,
            Self::Tier11 => 11,
            Self::Tier12 => 12,
            Self::Tier13 => 13,
            Self::Tier14 => 14,
            Self::Tier15 => 15,
            Self::Tier16 => 16,
            Self::Tier17 => 17,
            Self::Failed => 18,
        }
    }
}

/// A struct to hold both the calculated grade and the precise score percentage.
#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct CachedScore {
    pub grade: Grade,
    pub score_percent: f64, // Stored as 0.0 to 1.0
    /// Optional lamp index for UI (e.g., Select Music wheel).
    /// This is intentionally UI-agnostic: the meaning of the index is left
    /// to the presentation layer (colors, effects, etc.).
    pub lamp_index: Option<u8>,
    /// Optional single-digit judge count for the lamp (e.g. 1..=9).
    pub lamp_judge_count: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedItlScore {
    pub ex_hundredths: u32,
    pub clear_type: u8,
    pub points: u32,
}

// --- GrooveStats grade cache (on-disk + network-fetched) ---

#[derive(Default)]
struct GsScoreCacheState {
    loaded_profiles: HashMap<String, HashMap<String, CachedScore>>,
}

static GS_SCORE_CACHE: std::sync::LazyLock<Mutex<GsScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(GsScoreCacheState::default()));

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OnlineItlSelfScoreKey {
    chart_hash: String,
    api_key: String,
}

#[derive(Default)]
struct OnlineItlSelfScoreCacheState {
    by_key: HashMap<OnlineItlSelfScoreKey, u32>,
}

static ONLINE_ITL_SELF_SCORE_CACHE: std::sync::LazyLock<Mutex<OnlineItlSelfScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(OnlineItlSelfScoreCacheState::default()));

fn online_itl_self_score_key_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<OnlineItlSelfScoreKey> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || !profile::is_session_side_joined(side) {
        return None;
    }
    let side_profile = profile::get_for_side(side);
    let api_key = side_profile.groovestats_api_key.trim();
    if api_key.is_empty() {
        return None;
    }
    Some(OnlineItlSelfScoreKey {
        chart_hash: chart_hash.to_string(),
        api_key: api_key.to_string(),
    })
}

fn set_cached_online_itl_self_score(api_key: &str, chart_hash: &str, score: Option<u32>) {
    let api_key = api_key.trim();
    let chart_hash = chart_hash.trim();
    if api_key.is_empty() || chart_hash.is_empty() {
        return;
    }
    let key = OnlineItlSelfScoreKey {
        chart_hash: chart_hash.to_string(),
        api_key: api_key.to_string(),
    };
    let mut cache = ONLINE_ITL_SELF_SCORE_CACHE.lock().unwrap();
    if let Some(score) = score {
        cache.by_key.insert(key, score);
    } else {
        cache.by_key.remove(&key);
    }
}

fn gs_scores_dir_for_profile(profile_id: &str) -> PathBuf {
    PathBuf::from("save/profiles")
        .join(profile_id)
        .join("scores")
        .join("gs")
}

fn gs_scores_dir_for_profile_and_hash(profile_id: &str, chart_hash: &str) -> PathBuf {
    gs_scores_dir_for_profile(profile_id).join(shard2_for_hash(chart_hash))
}

fn gs_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    gs_scores_dir_for_profile(profile_id).join("index.bin")
}

fn load_gs_score_index(path: &Path) -> Option<HashMap<String, CachedScore>> {
    let bytes = fs::read(path).ok()?;
    let (by_chart, _) = bincode::decode_from_slice::<HashMap<String, CachedScore>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    Some(by_chart)
}

fn save_gs_score_index(path: &Path, by_chart: &HashMap<String, CachedScore>) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create GS score index dir {parent:?}: {e}");
        return;
    }
    let Ok(buf) = bincode::encode_to_vec(by_chart, bincode::config::standard()) else {
        warn!("Failed to encode GS score index at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write GS score index temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit GS score index file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
    }
}

fn ensure_gs_score_cache_loaded_for_profile(profile_id: &str) {
    let needs_load = {
        let state = GS_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }

    let load_started = Instant::now();
    let index_path = gs_score_index_path_for_profile(profile_id);
    let disk_cache = load_gs_score_index(&index_path).unwrap_or_else(|| {
        let scanned = best_scores_from_disk(&gs_scores_dir_for_profile(profile_id));
        save_gs_score_index(&index_path, &scanned);
        scanned
    });
    let loaded_entries = disk_cache.len();
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    if load_ms >= 25.0 {
        debug!(
            "Loaded GrooveStats score cache for profile {profile_id}: {loaded_entries} chart(s) in {load_ms:.2}ms."
        );
    }
    let mut state = GS_SCORE_CACHE.lock().unwrap();
    state
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(disk_cache);
}

pub fn get_cached_gs_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_gs_score_cache_loaded_for_profile(&profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|m| m.get(chart_hash).copied())
}

fn cached_gs_chart_hashes_for_profile(profile_id: &str) -> HashSet<String> {
    if profile_id.trim().is_empty() {
        return HashSet::new();
    }
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    GS_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(profile_id)
        .map_or_else(HashSet::new, |scores| scores.keys().cloned().collect())
}

fn set_cached_gs_score_for_profile(profile_id: &str, chart_hash: String, score: CachedScore) {
    debug!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
    ensure_gs_score_cache_loaded_for_profile(profile_id);
    let snapshot = {
        let mut state = GS_SCORE_CACHE.lock().unwrap();
        let Some(map) = state.loaded_profiles.get_mut(profile_id) else {
            return;
        };
        map.insert(chart_hash, score);
        map.clone()
    };
    save_gs_score_index(&gs_score_index_path_for_profile(profile_id), &snapshot);
}

// --- Local score cache (on-disk, one file per play) ---

#[derive(Clone, Copy, Debug, Encode, Decode)]
struct BestScalar {
    grade: Grade,
    percent: f64,
}

#[derive(Debug, Default, Clone, Encode, Decode)]
struct LocalScoreIndex {
    best_itg: HashMap<String, CachedScore>,
    best_ex: HashMap<String, BestScalar>,
    best_hard_ex: HashMap<String, BestScalar>,
}

#[derive(Default)]
struct LocalScoreCacheState {
    loaded_profiles: HashMap<String, LocalScoreIndex>,
}

static LOCAL_SCORE_CACHE: std::sync::LazyLock<Mutex<LocalScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(LocalScoreCacheState::default()));

#[derive(Clone, Debug)]
struct MachineBest {
    score: CachedScore,
    initials: String,
}

#[derive(Default)]
struct MachineLocalScoreCacheState {
    loaded: bool,
    best_itg: HashMap<String, MachineBest>,
}

static MACHINE_LOCAL_SCORE_CACHE: std::sync::LazyLock<Mutex<MachineLocalScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(MachineLocalScoreCacheState::default()));

#[derive(Default)]
struct ItlScoreCacheState {
    loaded_profiles: HashMap<String, ItlFileData>,
}

static ITL_SCORE_CACHE: std::sync::LazyLock<Mutex<ItlScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(ItlScoreCacheState::default()));

fn local_scores_root_for_profile(profile_id: &str) -> PathBuf {
    PathBuf::from("save/profiles")
        .join(profile_id)
        .join("scores")
        .join("local")
}

fn local_score_index_path_for_profile(profile_id: &str) -> PathBuf {
    local_scores_root_for_profile(profile_id).join("index.bin")
}

fn load_local_score_index_file(path: &Path) -> Option<LocalScoreIndex> {
    let bytes = fs::read(path).ok()?;
    let (index, _) =
        bincode::decode_from_slice::<LocalScoreIndex, _>(&bytes, bincode::config::standard())
            .ok()?;
    Some(index)
}

fn save_local_score_index_file(path: &Path, index: &LocalScoreIndex) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(e) = fs::create_dir_all(parent) {
        warn!("Failed to create local score index dir {parent:?}: {e}");
        return;
    }
    let Ok(buf) = bincode::encode_to_vec(index, bincode::config::standard()) else {
        warn!("Failed to encode local score index at {path:?}");
        return;
    };
    let tmp_path = path.with_extension("tmp");
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write local score index temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, path) {
        warn!("Failed to commit local score index file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
    }
}

fn count_local_score_bins_in_dir(dir: &Path) -> u32 {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return 0;
    };

    let mut total: u32 = 0;
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
        {
            total = total.saturating_add(1);
        }
    }
    total
}

/// Total number of locally-saved plays for this profile (one `.bin` per play).
pub fn total_songs_played_for_profile(profile_id: &str) -> u32 {
    let root = local_scores_root_for_profile(profile_id);
    if !root.is_dir() {
        return 0;
    }

    // Support both sharded (root/ab/*.bin) and unsharded (root/*.bin) layouts.
    let mut total = count_local_score_bins_in_dir(&root);
    let Ok(read_dir) = fs::read_dir(&root) else {
        return total;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(count_local_score_bins_in_dir(&path));
        }
    }
    total
}

pub fn total_songs_played_for_side(side: profile::PlayerSide) -> u32 {
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return 0;
    };
    total_songs_played_for_profile(&profile_id)
}

#[inline(always)]
fn parse_local_score_filename(name: &str) -> Option<(&str, i64)> {
    if !name.ends_with(".bin") {
        return None;
    }
    let base = &name[..name.len().saturating_sub(4)];
    let idx_dash = base.rfind('-')?;
    if idx_dash == 0 {
        return None;
    }
    let played_at_ms = base[(idx_dash + 1)..].parse::<i64>().ok()?;
    Some((&base[..idx_dash], played_at_ms))
}

fn collect_recent_plays_in_dir(dir: &Path, latest_by_chart: &mut HashMap<String, i64>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((chart_hash, played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        match latest_by_chart.get_mut(chart_hash) {
            Some(existing) => {
                if played_at_ms > *existing {
                    *existing = played_at_ms;
                }
            }
            None => {
                latest_by_chart.insert(chart_hash.to_string(), played_at_ms);
            }
        }
    }
}

fn collect_recent_plays_in_root(root: &Path, latest_by_chart: &mut HashMap<String, i64>) {
    collect_recent_plays_in_dir(root, latest_by_chart);
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recent_plays_in_dir(&path, latest_by_chart);
        }
    }
}

/// Returns chart hashes ordered by latest local play time (most recent first),
/// aggregated across all local profiles.
pub fn recent_played_chart_hashes_for_machine() -> Vec<String> {
    let profiles_root = PathBuf::from("save/profiles");
    let Ok(read_dir) = fs::read_dir(&profiles_root) else {
        return Vec::new();
    };

    let mut latest_by_chart: HashMap<String, i64> = HashMap::new();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_recent_plays_in_root(&local_root, &mut latest_by_chart);
        }
    }

    let mut ranked: Vec<(i64, String)> = latest_by_chart
        .into_iter()
        .map(|(chart_hash, played_at_ms)| (played_at_ms, chart_hash))
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    ranked
        .into_iter()
        .map(|(_, chart_hash)| chart_hash)
        .collect()
}

fn collect_play_counts_in_dir(dir: &Path, counts_by_chart: &mut HashMap<String, u32>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((chart_hash, _played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        counts_by_chart
            .entry(chart_hash.to_string())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
}

fn collect_play_counts_in_root(root: &Path, counts_by_chart: &mut HashMap<String, u32>) {
    collect_play_counts_in_dir(root, counts_by_chart);
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_play_counts_in_dir(&path, counts_by_chart);
        }
    }
}

/// Returns `(chart_hash, play_count)` pairs ordered by play count descending,
/// aggregated across all local profiles.
pub fn played_chart_counts_for_machine() -> Vec<(String, u32)> {
    let profiles_root = PathBuf::from("save/profiles");
    let Ok(read_dir) = fs::read_dir(&profiles_root) else {
        return Vec::new();
    };

    let mut counts_by_chart: HashMap<String, u32> = HashMap::new();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_play_counts_in_root(&local_root, &mut counts_by_chart);
        }
    }

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

#[inline(always)]
fn shard2_for_hash(hash: &str) -> &str {
    if hash.len() >= 2 { &hash[..2] } else { "00" }
}

#[inline(always)]
fn is_better_itg(new: &CachedScore, old: &CachedScore) -> bool {
    match (old.grade == Grade::Failed, new.grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    new.score_percent > old.score_percent
}

#[inline(always)]
fn is_better_scalar(new: BestScalar, old: BestScalar) -> bool {
    match (old.grade == Grade::Failed, new.grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    new.percent > old.percent
}

fn ensure_local_score_cache_loaded(profile_id: &str) {
    if LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .contains_key(profile_id)
    {
        return;
    }

    let load_started = Instant::now();
    let loaded = load_local_score_index(&local_scores_root_for_profile(profile_id));
    let loaded_itg = loaded.best_itg.len();
    let loaded_ex = loaded.best_ex.len();
    let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    if load_ms >= 25.0 {
        debug!(
            "Loaded local score cache for profile {profile_id}: ITG={loaded_itg}, EX={loaded_ex} in {load_ms:.2}ms."
        );
    }
    let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
    state
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(loaded);
}

fn profile_initials_for_id(profile_id: &str) -> Option<String> {
    let ini_path = PathBuf::from("save/profiles")
        .join(profile_id)
        .join("profile.ini");
    if !ini_path.is_file() {
        return None;
    }
    let mut ini = SimpleIni::new();
    if ini.load(&ini_path).is_err() {
        return None;
    }
    let s = ini.get("userprofile", "PlayerInitials")?;
    let s = s.trim();
    (!s.is_empty()).then_some(s.to_string())
}

fn ensure_machine_local_score_cache_loaded() {
    let needs_load = {
        let state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
        !state.loaded
    };
    if !needs_load {
        return;
    }

    let load_started = Instant::now();
    let mut best_itg: HashMap<String, MachineBest> = HashMap::new();
    for p in profile::scan_local_profiles() {
        let initials = profile_initials_for_id(&p.id).unwrap_or_else(|| "----".to_string());
        let idx = load_local_score_index(&local_scores_root_for_profile(&p.id));
        for (chart_hash, score) in idx.best_itg {
            match best_itg.get_mut(&chart_hash) {
                Some(existing) => {
                    if is_better_itg(&score, &existing.score) {
                        existing.score = score;
                        existing.initials = initials.clone();
                    }
                }
                None => {
                    best_itg.insert(
                        chart_hash,
                        MachineBest {
                            score,
                            initials: initials.clone(),
                        },
                    );
                }
            }
        }
    }

    let mut state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    if !state.loaded {
        state.loaded = true;
        state.best_itg = best_itg;
        let total = state.best_itg.len();
        let load_ms = load_started.elapsed().as_secs_f64() * 1000.0;
        if load_ms >= 25.0 {
            debug!("Loaded machine local score cache: {total} chart(s) in {load_ms:.2}ms.");
        }
    }
}

pub fn prewarm_select_music_score_caches() {
    let started = Instant::now();

    let p1_profile_id = profile::active_local_profile_id_for_side(profile::PlayerSide::P1);
    let p2_profile_id = profile::active_local_profile_id_for_side(profile::PlayerSide::P2);

    if let Some(profile_id) = p1_profile_id.as_deref() {
        ensure_local_score_cache_loaded(profile_id);
        ensure_gs_score_cache_loaded_for_profile(profile_id);
    }
    if let Some(profile_id) = p2_profile_id.as_deref()
        && p1_profile_id.as_deref() != Some(profile_id)
    {
        ensure_local_score_cache_loaded(profile_id);
        ensure_gs_score_cache_loaded_for_profile(profile_id);
    }

    ensure_machine_local_score_cache_loaded();

    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    debug!("Prewarmed SelectMusic score caches in {elapsed_ms:.2}ms.");
}

fn update_machine_cache_if_loaded(chart_hash: &str, score: CachedScore, initials: &str) {
    let mut state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    if !state.loaded {
        return;
    }
    match state.best_itg.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_itg(&score, &existing.score) {
                existing.score = score;
                existing.initials = initials.to_string();
            }
        }
        None => {
            state.best_itg.insert(
                chart_hash.to_string(),
                MachineBest {
                    score,
                    initials: initials.to_string(),
                },
            );
        }
    }
}

pub fn get_cached_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let local = get_cached_local_score_for_side(chart_hash, side);
    let gs = get_cached_gs_score_for_side(chart_hash, side);
    match (local, gs) {
        (None, None) => None,
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (Some(a), Some(b)) => Some(if is_better_itg(&a, &b) { a } else { b }),
    }
}

pub fn get_cached_local_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_local_score_cache_loaded(&profile_id);
    LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|idx| idx.best_itg.get(chart_hash).copied())
}

#[inline(always)]
pub fn is_gs_get_scores_service_allowed() -> bool {
    if !crate::config::get().enable_groovestats {
        return false;
    }
    matches!(
        network::get_status(),
        network::ConnectionStatus::Connected(services) if services.get_scores
    )
}

#[inline(always)]
pub fn is_gs_active_for_side(side: profile::PlayerSide) -> bool {
    if !is_gs_get_scores_service_allowed() || !profile::is_session_side_joined(side) {
        return false;
    }
    !profile::get_for_side(side)
        .groovestats_api_key
        .trim()
        .is_empty()
}

pub fn get_machine_record_local(chart_hash: &str) -> Option<(String, CachedScore)> {
    ensure_machine_local_score_cache_loaded();
    let state = MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    state
        .best_itg
        .get(chart_hash)
        .map(|m| (m.initials.clone(), m.score))
}

pub fn get_cached_itl_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|data| data.hash_map.get(chart_hash))
        .map(itl_score_from_entry)
}

pub fn get_cached_itl_score_for_song(
    song: &crate::game::song::SongData,
    side: profile::PlayerSide,
) -> Option<CachedItlScore> {
    let profile_id = profile::active_local_profile_id_for_side(side)?;
    ensure_itl_score_cache_loaded(&profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .get(&profile_id)
        .and_then(|data| itl_score_for_song(song, data))
}

// --- On-disk GrooveStats score storage ---

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntryV1 {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntry {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

const fn grade_to_code(g: Grade) -> u8 {
    match g {
        Grade::Quint => 0,
        Grade::Tier01 => 1,
        Grade::Tier02 => 2,
        Grade::Tier03 => 3,
        Grade::Tier04 => 4,
        Grade::Tier05 => 5,
        Grade::Tier06 => 6,
        Grade::Tier07 => 7,
        Grade::Tier08 => 8,
        Grade::Tier09 => 9,
        Grade::Tier10 => 10,
        Grade::Tier11 => 11,
        Grade::Tier12 => 12,
        Grade::Tier13 => 13,
        Grade::Tier14 => 14,
        Grade::Tier15 => 15,
        Grade::Tier16 => 16,
        Grade::Tier17 => 17,
        Grade::Failed => 18,
    }
}

const fn grade_from_code(code: u8) -> Grade {
    match code {
        0 => Grade::Quint,
        1 => Grade::Tier01,
        2 => Grade::Tier02,
        3 => Grade::Tier03,
        4 => Grade::Tier04,
        5 => Grade::Tier05,
        6 => Grade::Tier06,
        7 => Grade::Tier07,
        8 => Grade::Tier08,
        9 => Grade::Tier09,
        10 => Grade::Tier10,
        11 => Grade::Tier11,
        12 => Grade::Tier12,
        13 => Grade::Tier13,
        14 => Grade::Tier14,
        15 => Grade::Tier15,
        16 => Grade::Tier16,
        17 => Grade::Tier17,
        _ => Grade::Failed,
    }
}

fn entry_from_cached(score: CachedScore, username: &str, fetched_at_ms: i64) -> GsScoreEntry {
    GsScoreEntry {
        score_percent: score.score_percent,
        grade_code: grade_to_code(score.grade),
        lamp_index: score.lamp_index,
        lamp_judge_count: score.lamp_judge_count,
        username: username.to_string(),
        fetched_at_ms,
    }
}

fn cached_from_entry(entry: &GsScoreEntry) -> CachedScore {
    CachedScore {
        grade: grade_from_code(entry.grade_code),
        score_percent: entry.score_percent,
        lamp_index: entry.lamp_index,
        lamp_judge_count: entry.lamp_judge_count,
    }
}

fn decode_gs_score_entry(bytes: &[u8]) -> Option<GsScoreEntry> {
    if let Ok((entry, _)) =
        bincode::decode_from_slice::<GsScoreEntry, _>(bytes, bincode::config::standard())
    {
        return Some(entry);
    }
    if let Ok((v1, _)) =
        bincode::decode_from_slice::<GsScoreEntryV1, _>(bytes, bincode::config::standard())
    {
        return Some(GsScoreEntry {
            score_percent: v1.score_percent,
            grade_code: v1.grade_code,
            lamp_index: v1.lamp_index,
            lamp_judge_count: None,
            username: v1.username,
            fetched_at_ms: v1.fetched_at_ms,
        });
    }
    None
}

fn scan_gs_scores_dir(dir: &Path, best_by_chart: &mut HashMap<String, CachedScore>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".bin") {
            continue;
        }
        let base = &name[..name.len().saturating_sub(4)];
        let Some(idx) = base.rfind('-') else {
            continue;
        };
        if idx == 0 {
            continue;
        }
        let chart_hash = &base[..idx];

        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Some(entry) = decode_gs_score_entry(&bytes) else {
            continue;
        };
        let cached = cached_from_entry(&entry);

        match best_by_chart.get_mut(chart_hash) {
            Some(existing) => {
                if cached.score_percent > existing.score_percent {
                    *existing = cached;
                }
            }
            None => {
                best_by_chart.insert(chart_hash.to_string(), cached);
            }
        }
    }
}

fn best_scores_from_disk(dir: &Path) -> HashMap<String, CachedScore> {
    let mut best_by_chart: HashMap<String, CachedScore> = HashMap::new();

    if !dir.is_dir() {
        return best_by_chart;
    }

    // Sharded layout only: root/ab/*.bin
    let Ok(read_dir) = fs::read_dir(dir) else {
        return best_by_chart;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_gs_scores_dir(&path, &mut best_by_chart);
        }
    }

    best_by_chart
}

fn load_all_entries_for_chart(chart_hash: &str, dir: &Path) -> Vec<GsScoreEntry> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let prefix = format!("{chart_hash}-");
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".bin") {
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        if let Some(entry) = decode_gs_score_entry(&bytes) {
            entries.push(entry);
        }
    }
    entries
}

fn append_gs_score_on_disk_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
) {
    if username.trim().is_empty() {
        return;
    }
    let dir = gs_scores_dir_for_profile_and_hash(profile_id, chart_hash);

    let mut entries = load_all_entries_for_chart(chart_hash, &dir);
    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let new_entry = entry_from_cached(score, username, fetched_at_ms);

    let epsilon = 1e-9_f64;
    for existing in &entries {
        if existing.username.eq_ignore_ascii_case(username)
            && (existing.score_percent - new_entry.score_percent).abs() <= epsilon
            && existing.lamp_index == new_entry.lamp_index
            && existing.lamp_judge_count == new_entry.lamp_judge_count
            && existing.grade_code == new_entry.grade_code
        {
            return;
        }
    }

    entries.push(new_entry.clone());

    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create GrooveStats scores dir for profile {profile_id} at {dir:?}: {e}");
        return;
    }

    let file_name = format!("{chart_hash}-{fetched_at_ms}.bin");
    let path = dir.join(file_name);

    match bincode::encode_to_vec(&new_entry, bincode::config::standard()) {
        Ok(buf) => {
            if let Err(e) = fs::write(&path, buf) {
                warn!("Failed to write GrooveStats score file {path:?}: {e}");
            } else {
                debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
            }
        }
        Err(e) => {
            warn!("Failed to encode GrooveStats score for chart {chart_hash}: {e}");
        }
    }
}

// --- On-disk local score storage (one file per play) ---

const LOCAL_SCORE_VERSION_V1: u16 = 1;

#[derive(Debug, Clone, Copy, Encode, Decode)]
struct LocalReplayEdgeV1 {
    event_music_time: f32,
    lane: u8,
    pressed: bool,
    // 0 = Keyboard, 1 = Gamepad
    source: u8,
}

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreEntryHeaderV1 {
    version: u16,
    played_at_ms: i64,
    music_rate: f32,
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    ex_score_percent: f64,
    hard_ex_score_percent: f64,
    // Fantastic, Excellent, Great, Decent, WayOff, Miss (row judgments)
    judgment_counts: [u32; 6],
    holds_held: u32,
    holds_total: u32,
    rolls_held: u32,
    rolls_total: u32,
    mines_avoided: u32,
    mines_total: u32,
    hands_achieved: u32,
    fail_time: Option<f32>,
    beat0_time_seconds: f32,
}

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreEntryV1 {
    version: u16,
    played_at_ms: i64,
    music_rate: f32,
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
    ex_score_percent: f64,
    hard_ex_score_percent: f64,
    judgment_counts: [u32; 6],
    holds_held: u32,
    holds_total: u32,
    rolls_held: u32,
    rolls_total: u32,
    mines_avoided: u32,
    mines_total: u32,
    hands_achieved: u32,
    fail_time: Option<f32>,
    beat0_time_seconds: f32,
    replay: Vec<LocalReplayEdgeV1>,
}

#[inline(always)]
fn local_lamp_judge_count(count: u32) -> Option<u8> {
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

fn compute_local_lamp(counts: [u32; 6], grade: Grade) -> (Option<u8>, Option<u8>) {
    if grade == Grade::Failed {
        return (None, None);
    }
    if grade == Grade::Quint {
        return (Some(0), None);
    }

    let excellent = counts[1];
    let great = counts[2];
    let decent = counts[3];
    let wayoff = counts[4];
    let miss = counts[5];

    if miss == 0 && wayoff == 0 && decent == 0 && great == 0 && excellent == 0 {
        return (Some(1), None);
    }
    if miss == 0 && wayoff == 0 && decent == 0 && great == 0 {
        return (Some(2), local_lamp_judge_count(excellent));
    }
    if miss == 0 && wayoff == 0 && decent == 0 {
        return (Some(3), local_lamp_judge_count(great));
    }
    if miss == 0 && wayoff == 0 {
        return (Some(4), local_lamp_judge_count(decent));
    }
    (None, None)
}

fn decode_local_score_header(bytes: &[u8]) -> Option<LocalScoreEntryHeaderV1> {
    let Ok((h, _)) = bincode::decode_from_slice::<LocalScoreEntryHeaderV1, _>(
        bytes,
        bincode::config::standard(),
    ) else {
        return None;
    };
    if h.version != LOCAL_SCORE_VERSION_V1 {
        return None;
    }
    Some(h)
}

fn read_local_score_header(path: &Path) -> Option<LocalScoreEntryHeaderV1> {
    // Local score files include replay data; for indexing we only need the prefix.
    // A 1KiB prefix comfortably covers the fixed header fields.
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(1024);
    if file.take(1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return None;
    }
    decode_local_score_header(&buf)
}

fn read_local_score_entry(path: &Path) -> Option<LocalScoreEntryV1> {
    let bytes = fs::read(path).ok()?;
    let (entry, _) =
        bincode::decode_from_slice::<LocalScoreEntryV1, _>(&bytes, bincode::config::standard())
            .ok()?;
    if entry.version != LOCAL_SCORE_VERSION_V1 {
        return None;
    }
    Some(entry)
}

fn scan_local_scores_dir(dir: &Path, index: &mut LocalScoreIndex) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for item in read_dir.flatten() {
        let path = item.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.ends_with(".bin") {
            continue;
        }
        let base = &name[..name.len().saturating_sub(4)];
        let Some(idx_dash) = base.rfind('-') else {
            continue;
        };
        if idx_dash == 0 {
            continue;
        }
        let chart_hash = &base[..idx_dash];

        let Some(h) = read_local_score_header(&path) else {
            continue;
        };

        let grade = grade_from_code(h.grade_code);
        let cached = CachedScore {
            grade,
            score_percent: h.score_percent,
            lamp_index: h.lamp_index,
            lamp_judge_count: h.lamp_judge_count,
        };

        match index.best_itg.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_itg(&cached, existing) {
                    *existing = cached;
                }
            }
            None => {
                index.best_itg.insert(chart_hash.to_string(), cached);
            }
        }

        let ex = BestScalar {
            grade,
            percent: h.ex_score_percent,
        };
        match index.best_ex.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_scalar(ex, *existing) {
                    *existing = ex;
                }
            }
            None => {
                index.best_ex.insert(chart_hash.to_string(), ex);
            }
        }

        let hard_ex = BestScalar {
            grade,
            percent: h.hard_ex_score_percent,
        };
        match index.best_hard_ex.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_scalar(hard_ex, *existing) {
                    *existing = hard_ex;
                }
            }
            None => {
                index.best_hard_ex.insert(chart_hash.to_string(), hard_ex);
            }
        }
    }
}

fn load_local_score_index(root: &Path) -> LocalScoreIndex {
    if !root.is_dir() {
        return LocalScoreIndex::default();
    }
    let index_path = root.join("index.bin");
    if let Some(index) = load_local_score_index_file(&index_path) {
        return index;
    }

    let mut index = LocalScoreIndex::default();

    // Support both sharded (root/ab/*.bin) and unsharded (root/*.bin) layouts.
    scan_local_scores_dir(root, &mut index);
    let Ok(read_dir) = fs::read_dir(root) else {
        return index;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_local_scores_dir(&path, &mut index);
        }
    }

    save_local_score_index_file(&index_path, &index);
    index
}

fn update_local_index_with_header(
    idx: &mut LocalScoreIndex,
    chart_hash: &str,
    h: &LocalScoreEntryHeaderV1,
) {
    let grade = grade_from_code(h.grade_code);
    let cached = CachedScore {
        grade,
        score_percent: h.score_percent,
        lamp_index: h.lamp_index,
        lamp_judge_count: h.lamp_judge_count,
    };
    match idx.best_itg.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_itg(&cached, existing) {
                *existing = cached;
            }
        }
        None => {
            idx.best_itg.insert(chart_hash.to_string(), cached);
        }
    }

    let ex = BestScalar {
        grade,
        percent: h.ex_score_percent,
    };
    match idx.best_ex.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar(ex, *existing) {
                *existing = ex;
            }
        }
        None => {
            idx.best_ex.insert(chart_hash.to_string(), ex);
        }
    }

    let hard_ex = BestScalar {
        grade,
        percent: h.hard_ex_score_percent,
    };
    match idx.best_hard_ex.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar(hard_ex, *existing) {
                *existing = hard_ex;
            }
        }
        None => {
            idx.best_hard_ex.insert(chart_hash.to_string(), hard_ex);
        }
    }
}

fn append_local_score_on_disk(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntryV1,
) {
    let shard = shard2_for_hash(chart_hash);
    let dir = local_scores_root_for_profile(profile_id).join(shard);
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!("Failed to create local scores dir {dir:?}: {e}");
        return;
    }

    // Avoid collisions: keep the GS-like "<hash>-<ms>.bin" filename shape.
    let mut played_at_ms = entry.played_at_ms;
    let mut path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    while path.exists() {
        played_at_ms = played_at_ms.saturating_add(1);
        path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    }
    entry.played_at_ms = played_at_ms;

    let tmp_path = dir.join(format!(".{chart_hash}-{played_at_ms}.tmp"));
    let Ok(buf) = bincode::encode_to_vec(&*entry, bincode::config::standard()) else {
        warn!("Failed to encode local score for chart {chart_hash}");
        return;
    };
    if let Err(e) = fs::write(&tmp_path, buf) {
        warn!("Failed to write local score temp file {tmp_path:?}: {e}");
        return;
    }
    if let Err(e) = fs::rename(&tmp_path, &path) {
        warn!("Failed to commit local score file {path:?}: {e}");
        let _ = fs::remove_file(&tmp_path);
        return;
    }

    // Update in-memory cache if it's already loaded for this profile.
    let header = LocalScoreEntryHeaderV1 {
        version: entry.version,
        played_at_ms: entry.played_at_ms,
        music_rate: entry.music_rate,
        score_percent: entry.score_percent,
        grade_code: entry.grade_code,
        lamp_index: entry.lamp_index,
        lamp_judge_count: entry.lamp_judge_count,
        ex_score_percent: entry.ex_score_percent,
        hard_ex_score_percent: entry.hard_ex_score_percent,
        judgment_counts: entry.judgment_counts,
        holds_held: entry.holds_held,
        holds_total: entry.holds_total,
        rolls_held: entry.rolls_held,
        rolls_total: entry.rolls_total,
        mines_avoided: entry.mines_avoided,
        mines_total: entry.mines_total,
        hands_achieved: entry.hands_achieved,
        fail_time: entry.fail_time,
        beat0_time_seconds: entry.beat0_time_seconds,
    };
    let loaded_snapshot = {
        let mut state = LOCAL_SCORE_CACHE.lock().unwrap();
        if let Some(idx) = state.loaded_profiles.get_mut(profile_id) {
            update_local_index_with_header(idx, chart_hash, &header);
            Some(idx.clone())
        } else {
            None
        }
    };
    if let Some(index) = loaded_snapshot {
        save_local_score_index_file(&local_score_index_path_for_profile(profile_id), &index);
    } else {
        let index_path = local_score_index_path_for_profile(profile_id);
        let mut index = load_local_score_index_file(&index_path).unwrap_or_default();
        update_local_index_with_header(&mut index, chart_hash, &header);
        save_local_score_index_file(&index_path, &index);
    }

    let cached = CachedScore {
        grade: grade_from_code(header.grade_code),
        score_percent: header.score_percent,
        lamp_index: header.lamp_index,
        lamp_judge_count: header.lamp_judge_count,
    };
    update_machine_cache_if_loaded(chart_hash, cached, profile_initials);
}

fn judgment_counts_arr(p: &gameplay::PlayerRuntime) -> [u32; 6] {
    p.judgment_counts
}

fn replay_edges_for_player(gs: &gameplay::State, player: usize) -> Vec<LocalReplayEdgeV1> {
    if player >= gs.num_players {
        return Vec::new();
    }

    let (col_start, col_end) = if gs.num_players <= 1 {
        (0usize, gs.num_cols)
    } else {
        let start = player.saturating_mul(gs.cols_per_player);
        (start, start.saturating_add(gs.cols_per_player))
    };

    let mut out = Vec::new();
    out.reserve(gs.replay_edges.len().min(4096));
    for e in &gs.replay_edges {
        let lane = e.lane_index as usize;
        if lane < col_start || lane >= col_end || !e.event_music_time.is_finite() {
            continue;
        }
        let source = match e.source {
            InputSource::Keyboard => 0,
            InputSource::Gamepad => 1,
        };
        out.push(LocalReplayEdgeV1 {
            event_music_time: e.event_music_time,
            lane: (lane - col_start) as u8,
            pressed: e.pressed,
            source,
        });
    }
    out
}

pub fn save_local_scores_from_gameplay(gs: &gameplay::State) {
    if gs.autoplay_used {
        debug!("Skipping local score save: autoplay was used during this stage.");
        return;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Parameter retained for parity with Simply Love helpers; currently unused.
    let mines_disabled = false;

    for player_idx in 0..gs.num_players {
        if !gs.score_valid[player_idx] {
            debug!(
                "Skipping local score save for player {}: ranking-invalid modifiers were used.",
                player_idx + 1
            );
            continue;
        }

        let side = if gs.num_players >= 2 {
            if player_idx == 0 {
                profile::PlayerSide::P1
            } else {
                profile::PlayerSide::P2
            }
        } else {
            profile::get_session_player_side()
        };

        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };

        let chart_hash = gs.charts[player_idx].short_hash.as_str();
        let p = &gs.players[player_idx];

        let score_percent = judgment::calculate_itg_score_percent_from_counts(
            &p.scoring_counts,
            p.holds_held_for_score,
            p.rolls_held_for_score,
            p.mines_hit_for_score,
            gs.possible_grade_points[player_idx],
        );

        let mut grade = if p.is_failing || !gs.song_completed_naturally {
            Grade::Failed
        } else {
            score_to_grade(score_percent * 10000.0)
        };

        let (start, end) = gs.note_ranges[player_idx];
        let notes = &gs.notes[start..end];
        let note_times = &gs.note_time_cache[start..end];
        let hold_end_times = &gs.hold_end_time_cache[start..end];

        let ex_score_percent = judgment::calculate_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            p.fail_time,
            mines_disabled,
        );
        let hard_ex_score_percent = judgment::calculate_hard_ex_score_from_notes(
            notes,
            note_times,
            hold_end_times,
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            p.fail_time,
            mines_disabled,
        );

        // Simply Love: show Quint (Grade_Tier00) if EX score is exactly 100.00.
        if grade != Grade::Failed && ex_score_percent >= 100.0 {
            grade = Grade::Quint;
        }

        let counts = judgment_counts_arr(p);
        let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, grade);
        let replay = replay_edges_for_player(gs, player_idx);

        let mut entry = LocalScoreEntryV1 {
            version: LOCAL_SCORE_VERSION_V1,
            played_at_ms: now_ms,
            music_rate: gs.music_rate,
            score_percent,
            grade_code: grade_to_code(grade),
            lamp_index,
            lamp_judge_count,
            ex_score_percent,
            hard_ex_score_percent,
            judgment_counts: counts,
            holds_held: p.holds_held,
            holds_total: gs.holds_total[player_idx],
            rolls_held: p.rolls_held,
            rolls_total: gs.rolls_total[player_idx],
            mines_avoided: p.mines_avoided,
            mines_total: gs.mines_total[player_idx],
            hands_achieved: p.hands_achieved,
            fail_time: p.fail_time,
            beat0_time_seconds: gs.timing_players[player_idx].get_time_for_beat(0.0),
            replay,
        };

        append_local_score_on_disk(
            &profile_id,
            gs.player_profiles[player_idx].player_initials.as_str(),
            chart_hash,
            &mut entry,
        );
    }
}

pub fn save_itl_data_from_gameplay(
    gs: &gameplay::State,
) -> [Option<ItlEventProgress>; gameplay::MAX_PLAYERS] {
    let mut progress: [Option<ItlEventProgress>; gameplay::MAX_PLAYERS] =
        std::array::from_fn(|_| None);
    if gs.autoplay_used {
        debug!("Skipping ITL save: autoplay or replay was used during this stage.");
        return progress;
    }

    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
            continue;
        };
        let chart_hash = gs.charts[player_idx].short_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }

        let mut data = read_itl_file(profile_id.as_str());
        itl_rebuild_song_ranks(&mut data);
        let eval = itl_eval_state(gs, player_idx, &data);
        if !eval.active {
            continue;
        }
        if !eval.eligible {
            debug!(
                "Skipping ITL save for {:?} ({}): {}",
                side,
                chart_hash,
                eval.reason_lines.join("; ")
            );
            continue;
        }
        let prev_totals = itl_point_totals(&data);

        let Some(song_dir) = itl_song_dir(gs.song.as_ref()) else {
            continue;
        };
        let path_changed = data
            .path_map
            .get(song_dir.as_str())
            .is_none_or(|hash| !hash.eq_ignore_ascii_case(chart_hash));
        if path_changed {
            data.path_map
                .insert(song_dir.clone(), chart_hash.to_string());
        }

        let prev = data.hash_map.get(chart_hash).cloned();
        let (passing_points, max_scoring_points) =
            parse_itl_points(gs.charts[player_idx].chart_name.as_str())
                .or_else(|| {
                    prev.as_ref()
                        .map(|entry| (entry.passing_points, entry.max_scoring_points))
                })
                .unwrap_or((0, 0));
        let max_points = passing_points.saturating_add(max_scoring_points);
        let judgments = itl_judgments_from_gameplay(gs, player_idx);
        let (start, end) = gs.note_ranges[player_idx];
        let ex_percent = judgment::calculate_ex_score_from_notes(
            &gs.notes[start..end],
            &gs.note_time_cache[start..end],
            &gs.hold_end_time_cache[start..end],
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            gs.players[player_idx].fail_time,
            false,
        );
        let current_run_ex = itl_ex_hundredths(ex_percent);
        let new_entry = ItlHashEntry {
            judgments: judgments.clone(),
            ex: current_run_ex,
            clear_type: itl_clear_type(&judgments),
            points: itl_points_for_song(passing_points, max_scoring_points, ex_percent),
            used_cmod: eval.used_cmod,
            date: Local::now().format("%Y-%m-%d").to_string(),
            no_cmod: eval.chart_no_cmod,
            passing_points,
            max_scoring_points,
            max_points,
            rank: None,
            steps_type: itl_steps_type(gs.charts[player_idx].as_ref()).to_string(),
            passes: prev
                .as_ref()
                .map_or(1, |entry| entry.passes.saturating_add(1)),
        };

        let mut needs_write = path_changed;
        let mut best_changed = false;
        match data.hash_map.get_mut(chart_hash) {
            None => {
                data.hash_map
                    .insert(chart_hash.to_string(), new_entry.clone());
                needs_write = true;
            }
            Some(existing) => {
                if existing.passes != new_entry.passes {
                    existing.passes = new_entry.passes;
                    needs_write = true;
                }
                if !existing
                    .steps_type
                    .eq_ignore_ascii_case(new_entry.steps_type.as_str())
                {
                    existing.steps_type = new_entry.steps_type.clone();
                    needs_write = true;
                }

                let ex_improved = new_entry.ex > existing.ex;
                let ex_tied = new_entry.ex == existing.ex;
                if ex_improved {
                    existing.ex = new_entry.ex;
                    existing.points = new_entry.points;
                    existing.judgments = new_entry.judgments.clone();
                    needs_write = true;
                    best_changed = true;
                } else if ex_tied && itl_judgments_better(&new_entry.judgments, &existing.judgments)
                {
                    existing.judgments = new_entry.judgments.clone();
                    needs_write = true;
                    best_changed = true;
                }
                if new_entry.clear_type > existing.clear_type {
                    existing.clear_type = new_entry.clear_type;
                    needs_write = true;
                    best_changed = true;
                }
                if best_changed {
                    existing.used_cmod = new_entry.used_cmod;
                    existing.date = new_entry.date.clone();
                    existing.no_cmod = new_entry.no_cmod;
                    existing.passing_points = new_entry.passing_points;
                    existing.max_scoring_points = new_entry.max_scoring_points;
                    existing.max_points = new_entry.max_points;
                }
            }
        }

        itl_rebuild_song_ranks(&mut data);
        let current_totals = itl_point_totals(&data);
        let current_entry = data
            .hash_map
            .get(chart_hash)
            .cloned()
            .unwrap_or(new_entry.clone());
        let prev_entry = prev.unwrap_or_default();
        let mut event_progress = ItlEventProgress {
            name: itl_event_name(gs.song.as_ref()),
            is_doubles: current_entry.steps_type.eq_ignore_ascii_case("double"),
            score_hundredths: current_run_ex,
            score_delta_hundredths: itl_delta_i32(current_run_ex, prev_entry.ex),
            current_points: current_entry.points,
            point_delta: itl_delta_i32(current_entry.points, prev_entry.points),
            current_ranking_points: current_totals.ranking_points,
            ranking_delta: itl_delta_i32(current_totals.ranking_points, prev_totals.ranking_points),
            current_song_points: current_totals.song_points,
            song_delta: itl_delta_i32(current_totals.song_points, prev_totals.song_points),
            current_ex_points: current_totals.ex_points,
            ex_delta: itl_delta_i32(current_totals.ex_points, prev_totals.ex_points),
            current_total_points: current_totals.total_points,
            total_delta: itl_delta_i32(current_totals.total_points, prev_totals.total_points),
            total_passes: current_entry.passes.max(1),
            clear_type_before: Some(prev_entry.clear_type),
            clear_type_after: Some(current_entry.clear_type),
            overlay_pages: Vec::new(),
        };
        event_progress.overlay_pages = itl_overlay_pages(&event_progress, None, &[]);
        progress[player_idx] = Some(event_progress);

        if needs_write {
            write_itl_file(profile_id.as_str(), &data);
            set_cached_itl_file(profile_id.as_str(), data);
        }
    }

    progress
}

const GROOVESTATS_SUBMIT_MAX_ENTRIES: usize = 10;
const GROOVESTATS_COMMENT_PREFIX: &str = "[DS]";
// Mirrors zmod's old-api submit path bit layout from gameplay.rs/player options.
const GS_INVALID_REMOVE_MASK: u8 =
    (1u8 << 0) | (1u8 << 2) | (1u8 << 3) | (1u8 << 4) | (1u8 << 5) | (1u8 << 6) | (1u8 << 7);
const GS_INVALID_INSERT_MASK: u8 = u8::MAX;
const GS_INVALID_HOLDS_MASK: u8 = 1u8 << 3;
const GROOVESTATS_REASON_COUNT: usize = 13;
const GROOVESTATS_CHART_HASH_VERSION: u8 = 3;
const ITL_FILE_NAME: &str = "ITL2026.json";

#[derive(Clone, Debug, Default)]
pub struct GrooveStatsEvalState {
    pub valid: bool,
    pub reason_lines: Vec<String>,
    pub manual_qr_url: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ItlEvalState {
    pub active: bool,
    pub eligible: bool,
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub reason_lines: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum ItlOverlayPage {
    Text(String),
    Leaderboard(Vec<LeaderboardEntry>),
}

#[derive(Clone, Debug, Default)]
pub struct ItlEventProgress {
    pub name: String,
    pub is_doubles: bool,
    pub score_hundredths: u32,
    pub score_delta_hundredths: i32,
    pub current_points: u32,
    pub point_delta: i32,
    pub current_ranking_points: u32,
    pub ranking_delta: i32,
    pub current_song_points: u32,
    pub song_delta: i32,
    pub current_ex_points: u32,
    pub ex_delta: i32,
    pub current_total_points: u32,
    pub total_delta: i32,
    pub total_passes: u32,
    pub clear_type_before: Option<u8>,
    pub clear_type_after: Option<u8>,
    pub overlay_pages: Vec<ItlOverlayPage>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct ItlFileData {
    #[serde(rename = "pathMap", default)]
    path_map: HashMap<String, String>,
    #[serde(rename = "hashMap", default)]
    hash_map: HashMap<String, ItlHashEntry>,
    #[serde(default)]
    points: Vec<u32>,
    #[serde(rename = "pointsSingle", default)]
    points_single: Vec<u32>,
    #[serde(rename = "pointsDouble", default)]
    points_double: Vec<u32>,
    #[serde(rename = "unlockFolders", default)]
    unlock_folders: HashMap<String, bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ItlHashEntry {
    #[serde(default)]
    judgments: ItlJudgments,
    #[serde(default, deserialize_with = "deserialize_itl_ex")]
    ex: u32,
    #[serde(rename = "clearType", default)]
    clear_type: u8,
    #[serde(default)]
    points: u32,
    #[serde(rename = "usedCmod", default)]
    used_cmod: bool,
    #[serde(default)]
    date: String,
    #[serde(rename = "noCmod", default)]
    no_cmod: bool,
    #[serde(rename = "passingPoints", default)]
    passing_points: u32,
    #[serde(rename = "maxScoringPoints", default)]
    max_scoring_points: u32,
    #[serde(rename = "maxPoints", default)]
    max_points: u32,
    #[serde(default)]
    rank: Option<u32>,
    #[serde(rename = "stepsType", default)]
    steps_type: String,
    #[serde(default)]
    passes: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ItlJudgments {
    #[serde(rename = "W0", default)]
    w0: u32,
    #[serde(rename = "W1", default)]
    w1: u32,
    #[serde(rename = "W2", default)]
    w2: u32,
    #[serde(rename = "W3", default)]
    w3: u32,
    #[serde(rename = "W4", default)]
    w4: u32,
    #[serde(rename = "W5", default)]
    w5: u32,
    #[serde(rename = "Miss", default)]
    miss: u32,
    #[serde(rename = "totalSteps", default)]
    total_steps: u32,
    #[serde(rename = "Holds", default)]
    holds: u32,
    #[serde(rename = "totalHolds", default)]
    total_holds: u32,
    #[serde(rename = "Mines", default)]
    mines: u32,
    #[serde(rename = "totalMines", default)]
    total_mines: u32,
    #[serde(rename = "Rolls", default)]
    rolls: u32,
    #[serde(rename = "totalRolls", default)]
    total_rolls: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ItlPointTotals {
    ranking_points: u32,
    song_points: u32,
    ex_points: u32,
    total_points: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsSubmitUiStatus {
    Submitting,
    Submitted,
    SubmitFailed,
    TimedOut,
}

#[derive(Debug, Clone)]
struct GrooveStatsSubmitUiEntry {
    chart_hash: String,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
}

static GROOVESTATS_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<[Option<GrooveStatsSubmitUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));
static GROOVESTATS_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
struct GrooveStatsSubmitEventUiEntry {
    chart_hash: String,
    token: u64,
    itl_progress: Option<ItlEventProgress>,
}

static GROOVESTATS_SUBMIT_EVENT_UI: std::sync::LazyLock<
    Mutex<[Option<GrooveStatsSubmitEventUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsJudgmentCounts {
    fantastic_plus: u32,
    fantastic: u32,
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
    miss: u32,
    total_steps: u32,
    holds_held: u32,
    total_holds: u32,
    mines_hit: u32,
    total_mines: u32,
    rolls_held: u32,
    total_rolls: u32,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsRescoreCounts {
    fantastic_plus: u32,
    fantastic: u32,
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitPlayerPayload {
    rate: u32,
    score: u32,
    judgment_counts: GrooveStatsJudgmentCounts,
    rescore_counts: GrooveStatsRescoreCounts,
    used_cmod: bool,
    comment: String,
}

#[derive(Debug)]
struct GrooveStatsSubmitPlayerJob {
    side: profile::PlayerSide,
    slot: u8,
    chart_hash: String,
    username: String,
    profile_name: String,
    profile_id: Option<String>,
    token: u64,
    itl_score_hundredths: Option<u32>,
}

#[derive(Debug)]
struct GrooveStatsSubmitRequest {
    players: Vec<GrooveStatsSubmitPlayerJob>,
    headers: Vec<(String, String)>,
    query: Vec<(String, String)>,
    body: JsonValue,
}

#[derive(Debug)]
struct GrooveStatsSubmitError {
    status: GrooveStatsSubmitUiStatus,
    message: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiResponse {
    #[serde(default)]
    error: String,
    player1: Option<GrooveStatsSubmitApiPlayer>,
    player2: Option<GrooveStatsSubmitApiPlayer>,
}

impl GrooveStatsSubmitApiResponse {
    #[inline(always)]
    fn player_for_slot(&self, slot: u8) -> Option<&GrooveStatsSubmitApiPlayer> {
        match slot {
            1 => self.player1.as_ref(),
            2 => self.player2.as_ref(),
            _ => None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiPlayer {
    #[serde(default)]
    chart_hash: String,
    #[serde(default)]
    result: String,
    #[serde(rename = "gsLeaderboard", default)]
    gs_leaderboard: Vec<LeaderboardApiEntry>,
    rpg: Option<GrooveStatsSubmitApiEvent>,
    itl: Option<GrooveStatsSubmitApiEvent>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiEvent {
    #[serde(default)]
    name: String,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    score_delta: i32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    prev_top_score_points: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    total_passes: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    current_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    previous_ranking_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    current_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    previous_song_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    current_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    previous_ex_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    current_point_total: u32,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    previous_point_total: u32,
    #[serde(rename = "itlLeaderboard", default)]
    itl_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(default)]
    is_doubles: bool,
    progress: Option<GrooveStatsSubmitApiProgress>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiProgress {
    #[serde(rename = "statImprovements", default)]
    stat_improvements: Vec<GrooveStatsSubmitApiStatImprovement>,
    #[serde(rename = "questsCompleted", default)]
    quests_completed: Vec<GrooveStatsSubmitApiQuest>,
    #[serde(rename = "achievementsCompleted", default)]
    achievements_completed: Vec<GrooveStatsSubmitApiAchievement>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiStatImprovement {
    #[serde(default)]
    name: String,
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    gained: u32,
    #[serde(default, deserialize_with = "de_i32_from_string_or_number")]
    current: i32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiQuest {
    #[serde(default)]
    title: String,
    #[serde(default)]
    rewards: Vec<GrooveStatsSubmitApiQuestReward>,
    #[serde(default)]
    song_download_url: String,
    #[serde(rename = "songDownloadFolders", default)]
    song_download_folders: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiQuestReward {
    #[serde(rename = "type", default)]
    reward_type: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiAchievement {
    #[serde(default)]
    title: String,
    #[serde(default)]
    rewards: Vec<GrooveStatsSubmitApiAchievementReward>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GrooveStatsSubmitApiAchievementReward {
    #[serde(default, deserialize_with = "de_string_from_string_or_number")]
    tier: String,
    #[serde(default)]
    requirements: Vec<String>,
    #[serde(rename = "titleUnlocked", default)]
    title_unlocked: String,
}

#[inline(always)]
fn groovestats_reset_submit_ui_status(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap();
    let slot = &mut state[arrowcloud_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn groovestats_reset_submit_event_ui(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap();
    let slot = &mut state[arrowcloud_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn groovestats_set_submit_ui_status(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap()[arrowcloud_side_ix(side)] =
        Some(GrooveStatsSubmitUiEntry {
            chart_hash: hash.to_string(),
            token,
            status,
        });
}

#[inline(always)]
fn groovestats_update_submit_ui_status_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    let mut state = GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap();
    let Some(entry) = state[arrowcloud_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.status = status;
}

#[inline(always)]
fn groovestats_arm_submit_event_ui(side: profile::PlayerSide, chart_hash: &str, token: u64) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap()[arrowcloud_side_ix(side)] =
        Some(GrooveStatsSubmitEventUiEntry {
            chart_hash: hash.to_string(),
            token,
            itl_progress: None,
        });
}

#[inline(always)]
fn groovestats_update_submit_event_ui_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    itl_progress: Option<ItlEventProgress>,
) {
    let mut state = GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap();
    let Some(entry) = state[arrowcloud_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.itl_progress = itl_progress;
}

#[inline(always)]
fn groovestats_next_submit_ui_token() -> u64 {
    GROOVESTATS_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

pub fn get_groovestats_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<GrooveStatsSubmitUiStatus> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_UI_STATUS.lock().unwrap()[arrowcloud_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .map(|entry| entry.status)
}

pub fn get_groovestats_submit_itl_progress_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<ItlEventProgress> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap()[arrowcloud_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .and_then(|entry| entry.itl_progress.clone())
}

#[inline(always)]
fn compact_f32_text(value: f32) -> String {
    let mut text = format!("{value:.2}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[inline(always)]
fn current_itl_score_hundredths(gs: &gameplay::State, player_idx: usize) -> u32 {
    let (start, end) = gs.note_ranges[player_idx];
    let ex_percent = judgment::calculate_ex_score_from_notes(
        &gs.notes[start..end],
        &gs.note_time_cache[start..end],
        &gs.hold_end_time_cache[start..end],
        gs.total_steps[player_idx],
        gs.holds_total[player_idx],
        gs.rolls_total[player_idx],
        gs.mines_total[player_idx],
        gs.players[player_idx].fail_time,
        false,
    );
    itl_ex_hundredths(ex_percent)
}

#[inline(always)]
fn groovestats_submit_url() -> String {
    format!(
        "{}/score-submit.php",
        network::groovestats_api_base_url().trim_end_matches('/')
    )
}

fn groovestats_reason_lines(
    checks: &[bool; GROOVESTATS_REASON_COUNT],
    bad: &[String],
) -> Vec<String> {
    let mut out = Vec::with_capacity(6);
    for (idx, passed) in checks.iter().enumerate() {
        if *passed {
            continue;
        }
        match idx {
            0 => out.push("GrooveStats only supports dance and pump charts.".to_string()),
            1 => out.push("GrooveStats does not support dance-solo charts.".to_string()),
            2 => out.push("GrooveStats QR is unavailable in course mode.".to_string()),
            3 => out.push("GrooveStats requires ITG mode.".to_string()),
            4 => out.push("Timing windows must be at ITG or harder.".to_string()),
            5 => out.push("Life difficulty must be at ITG or harder.".to_string()),
            6 => {
                out.push("Metrics or preferences are incorrect.".to_string());
                out.extend(bad.iter().cloned());
            }
            7 => out.push("Music rate must be between 1.0x and 3.0x.".to_string()),
            8 => out.push("Note-removal modifiers are enabled.".to_string()),
            9 => out.push("Note-insertion modifiers are enabled.".to_string()),
            10 => out.push("Fail type must be Immediate or ImmediateContinue.".to_string()),
            11 => out.push("Autoplay or replay is not allowed.".to_string()),
            12 => out.push("MinTNSToScoreNotes cannot be W1 or W2.".to_string()),
            _ => {}
        }
    }
    out
}

fn groovestats_eval_state(
    chart: &crate::game::chart::ChartData,
    profile: &Profile,
    music_rate: f32,
    autoplay_used: bool,
    is_course_mode: bool,
) -> GrooveStatsEvalState {
    let chart_type = chart.chart_type.trim().to_ascii_lowercase();
    let rate = if music_rate.is_finite() && music_rate > 0.0 {
        music_rate
    } else {
        1.0
    };
    let remove_mask = profile::normalize_remove_mask(profile.remove_active_mask);
    let insert_mask = profile::normalize_insert_mask(profile.insert_active_mask);
    let holds_mask = profile::normalize_holds_mask(profile.holds_active_mask);
    let fail_type_ok = matches!(
        crate::config::get().default_fail_type,
        crate::config::DefaultFailType::Immediate
            | crate::config::DefaultFailType::ImmediateContinue
    );

    let mut checks = [true; GROOVESTATS_REASON_COUNT];
    checks[0] = chart_type.starts_with("dance") || chart_type.starts_with("pump");
    checks[1] = !chart_type.contains("solo");
    checks[2] = !is_course_mode;
    checks[3] = true;
    checks[4] = true;
    checks[5] = true;
    checks[6] = !profile.custom_fantastic_window;
    checks[7] = (1.0..=3.0).contains(&rate);
    checks[8] = (remove_mask & GS_INVALID_REMOVE_MASK) == 0;
    checks[9] = (insert_mask & GS_INVALID_INSERT_MASK) == 0;
    checks[10] = fail_type_ok;
    checks[11] = !autoplay_used;
    checks[12] = true;
    if (holds_mask & GS_INVALID_HOLDS_MASK) != 0 {
        checks[8] = false;
    }

    let mut bad = Vec::with_capacity(1);
    if profile.custom_fantastic_window {
        bad.push(format!(
            "- Custom Fantastic window ({}ms)",
            profile.custom_fantastic_window_ms
        ));
    }

    GrooveStatsEvalState {
        valid: checks.iter().all(|passed| *passed),
        reason_lines: groovestats_reason_lines(&checks, bad.as_slice()),
        manual_qr_url: None,
    }
}

#[inline(always)]
fn groovestats_qr_append_rescore(out: &mut String, label: char, value: u32) {
    if value == 0 {
        return;
    }
    out.push(label);
    out.push_str(format!("{value:x}").as_str());
}

fn groovestats_manual_qr_url(
    base_url: &str,
    chart_hash: &str,
    hash_version: u8,
    counts: &GrooveStatsJudgmentCounts,
    rescored: &GrooveStatsRescoreCounts,
    failed: bool,
    rate: u32,
    used_cmod: bool,
) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }

    let mut rescored_str = String::with_capacity(24);
    for (label, value) in [
        ('G', rescored.fantastic_plus),
        ('H', rescored.fantastic),
        ('I', rescored.excellent),
        ('J', rescored.great),
        ('K', rescored.decent),
        ('L', rescored.way_off),
    ] {
        groovestats_qr_append_rescore(&mut rescored_str, label, value);
    }

    Some(format!(
        "{}/QR/{hash}/T{:x}G{:x}H{:x}I{:x}J{:x}K{:x}L{:x}M{:x}H{:x}T{:x}R{:x}T{:x}M{:x}T{:x}{rescored_str}/F{}R{:x}C{}V{:x}",
        base_url.trim_end_matches('/'),
        counts.total_steps,
        counts.fantastic_plus,
        counts.fantastic,
        counts.excellent,
        counts.great,
        counts.decent,
        counts.way_off,
        counts.miss,
        counts.holds_held,
        counts.total_holds,
        counts.rolls_held,
        counts.total_rolls,
        counts.mines_hit,
        counts.total_mines,
        if failed { '1' } else { '0' },
        rate,
        if used_cmod { '1' } else { '0' },
        hash_version,
    ))
}

fn groovestats_manual_qr_url_from_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<String> {
    if player_idx >= gs.num_players {
        return None;
    }
    let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
        return None;
    };
    groovestats_manual_qr_url(
        network::groovestats_qr_base_url(),
        gs.charts[player_idx].short_hash.as_str(),
        GROOVESTATS_CHART_HASH_VERSION,
        &payload.judgment_counts,
        &payload.rescore_counts,
        gs.players[player_idx].fail_time.is_some() || gs.players[player_idx].is_failing,
        payload.rate,
        payload.used_cmod,
    )
}

pub fn groovestats_eval_state_from_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
) -> GrooveStatsEvalState {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS) {
        return GrooveStatsEvalState::default();
    }
    let mut state = groovestats_eval_state(
        gs.charts[player_idx].as_ref(),
        &gs.player_profiles[player_idx],
        gs.music_rate,
        gs.autoplay_used,
        gs.course_display_totals.is_some(),
    );
    if state.valid {
        state.manual_qr_url = groovestats_manual_qr_url_from_gameplay(gs, player_idx);
    }
    state
}

fn itl_file_path(profile_id: &str) -> PathBuf {
    profile::local_profile_dir_for_id(profile_id).join(ITL_FILE_NAME)
}

fn ensure_itl_score_cache_loaded(profile_id: &str) {
    let needs_load = {
        let state = ITL_SCORE_CACHE.lock().unwrap();
        !state.loaded_profiles.contains_key(profile_id)
    };
    if !needs_load {
        return;
    }

    let data = read_itl_file(profile_id);
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .entry(profile_id.to_string())
        .or_insert(data);
}

fn set_cached_itl_file(profile_id: &str, data: ItlFileData) {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .loaded_profiles
        .insert(profile_id.to_string(), data);
}

fn deserialize_itl_ex<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Option::<f64>::deserialize(deserializer)?.unwrap_or(0.0);
    if !raw.is_finite() || raw <= 0.0 {
        return Ok(0);
    }
    let scaled = if raw <= 100.0001 { raw * 100.0 } else { raw };
    Ok(scaled.round().clamp(0.0, 10_000.0) as u32)
}

#[inline(always)]
fn itl_ex_hundredths(ex_percent: f64) -> u32 {
    let ex = if ex_percent.is_finite() {
        ex_percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    (ex * 100.0).round() as u32
}

fn read_itl_file(profile_id: &str) -> ItlFileData {
    let path = itl_file_path(profile_id);
    let Ok(text) = fs::read_to_string(&path) else {
        return ItlFileData::default();
    };
    serde_json::from_str(text.as_str()).unwrap_or_else(|error| {
        warn!("Failed to parse ITL data file {path:?}: {error}");
        ItlFileData::default()
    })
}

fn write_itl_file(profile_id: &str, data: &ItlFileData) {
    if data.path_map.is_empty() && data.hash_map.is_empty() && data.unlock_folders.is_empty() {
        return;
    }
    let path = itl_file_path(profile_id);
    let Some(parent) = path.parent() else {
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        warn!("Failed to create ITL profile dir {parent:?}: {error}");
        return;
    }
    let Ok(text) = serde_json::to_string(data) else {
        warn!("Failed to encode ITL data for profile {profile_id}");
        return;
    };
    let tmp = path.with_extension("tmp");
    if let Err(error) = fs::write(&tmp, text) {
        warn!("Failed to write ITL temp file {tmp:?}: {error}");
        return;
    }
    if let Err(error) = fs::rename(&tmp, &path) {
        warn!("Failed to commit ITL file {path:?}: {error}");
        let _ = fs::remove_file(&tmp);
    }
}

fn update_itl_unlock_folders(profile_id: &str, folders: &[String]) {
    if folders.is_empty() {
        return;
    }
    let mut data = read_itl_file(profile_id);
    let mut changed = false;
    for folder in folders {
        let folder = folder.trim();
        if folder.is_empty() {
            continue;
        }
        changed |= data.unlock_folders.insert(folder.to_string(), true) != Some(true);
    }
    if changed {
        write_itl_file(profile_id, &data);
        set_cached_itl_file(profile_id, data);
    }
}

fn event_name_or_unknown(name: &str) -> &str {
    if name.trim().is_empty() {
        "Unknown Event"
    } else {
        name.trim()
    }
}

#[inline(always)]
fn itl_clear_type_name(clear_type: u8) -> &'static str {
    match clear_type {
        0 => "No Play",
        1 => "Clear",
        2 => "FC",
        3 => "FEC",
        4 => "FFC",
        5 => "FBFC",
        _ => "Clear",
    }
}

fn trim_blank_lines(text: String) -> String {
    text.trim_end_matches(['\n', '\r']).to_string()
}

fn capitalize_ascii_first(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out = String::new();
    out.extend(first.to_uppercase());
    out.extend(chars);
    out
}

fn itl_stat_improvement_lines(progress: Option<&GrooveStatsSubmitApiProgress>) -> Vec<String> {
    let Some(progress) = progress else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 {
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("clearType") {
            let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
            let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
            lines.push(format!(
                "Clear Type: {} >>> {}",
                itl_clear_type_name(before),
                itl_clear_type_name(after)
            ));
            continue;
        }
        if improvement.name.eq_ignore_ascii_case("grade") {
            let curr = improvement.current;
            let prev = curr - improvement.gained as i32;
            if curr != 0 && prev != curr {
                let grade = match curr {
                    1 => Some("Quad"),
                    2 => Some("Quint"),
                    _ => None,
                };
                if let Some(grade) = grade {
                    lines.push(format!("New {grade}!"));
                }
            }
            continue;
        }
        let stat_name = capitalize_ascii_first(improvement.name.trim_end_matches("Level"));
        lines.push(format!(
            "{stat_name} Lvl: {} (+{})",
            improvement.current, improvement.gained
        ));
    }
    lines
}

fn itl_summary_page_text(
    progress: &ItlEventProgress,
    submit_progress: Option<&GrooveStatsSubmitApiProgress>,
) -> String {
    let mut text = format!(
        "EX Score: {:.2}% ({:+.2}%)\n\
         Points: {} ({:+})\n\n\
         Ranking Points: {} ({:+})\n\
         Song Points: {} ({:+})\n\
         EX Points: {} ({:+})\n\
         Total Points: {} ({:+})\n\n\
         You've passed the chart {} times",
        progress.score_hundredths as f64 / 100.0,
        progress.score_delta_hundredths as f64 / 100.0,
        progress.current_points,
        progress.point_delta,
        progress.current_ranking_points,
        progress.ranking_delta,
        progress.current_song_points,
        progress.song_delta,
        progress.current_ex_points,
        progress.ex_delta,
        progress.current_total_points,
        progress.total_delta,
        progress.total_passes,
    );
    let lines = itl_stat_improvement_lines(submit_progress);
    if !lines.is_empty() {
        text.push_str("\n\n");
        text.push_str(lines.join("\n").as_str());
    }
    trim_blank_lines(text)
}

fn append_grouped_reward_text(out: &mut String, reward_type: &str, descriptions: &[String]) {
    if descriptions.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    if !reward_type.eq_ignore_ascii_case("ad-hoc") {
        out.push_str(reward_type.trim().to_ascii_uppercase().as_str());
        out.push_str(":\n");
    }
    out.push_str(descriptions.join("\n").as_str());
}

fn itl_quest_page_text(quest: &GrooveStatsSubmitApiQuest) -> String {
    let mut body = format!("Completed \"{}\"!", quest.title.trim());
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    for reward in &quest.rewards {
        let reward_type = reward.reward_type.trim();
        let description = reward.description.trim();
        if description.is_empty() {
            continue;
        }
        if let Some((_, descriptions)) = grouped
            .iter_mut()
            .find(|(kind, _)| kind.eq_ignore_ascii_case(reward_type))
        {
            descriptions.push(description.to_string());
        } else {
            grouped.push((reward_type.to_string(), vec![description.to_string()]));
        }
    }
    for (reward_type, descriptions) in &grouped {
        append_grouped_reward_text(&mut body, reward_type.as_str(), descriptions.as_slice());
    }
    trim_blank_lines(body)
}

fn itl_achievement_page_text(achievement: &GrooveStatsSubmitApiAchievement) -> String {
    let mut lines = vec![format!(
        "Completed the \"{}\" Achievement!",
        achievement.title.trim()
    )];
    for reward in &achievement.rewards {
        let tier = reward.tier.trim();
        if !tier.is_empty() && tier != "0" {
            lines.push(format!("Tier {tier}"));
        }
        for requirement in &reward.requirements {
            let requirement = requirement.trim();
            if !requirement.is_empty() {
                lines.push(requirement.to_string());
            }
        }
        let title = reward.title_unlocked.trim();
        if !title.is_empty() {
            lines.push(format!("Unlocked the \"{}\" Title!", title));
        }
        lines.push(String::new());
    }
    trim_blank_lines(lines.join("\n"))
}

fn itl_overlay_pages(
    progress: &ItlEventProgress,
    submit_progress: Option<&GrooveStatsSubmitApiProgress>,
    submit_leaderboard: &[LeaderboardApiEntry],
) -> Vec<ItlOverlayPage> {
    let mut pages = vec![ItlOverlayPage::Text(itl_summary_page_text(
        progress,
        submit_progress,
    ))];
    let Some(submit_progress) = submit_progress else {
        pages.push(ItlOverlayPage::Leaderboard(leaderboard_entries_from_api(
            submit_leaderboard.to_vec(),
        )));
        return pages;
    };
    for quest in &submit_progress.quests_completed {
        pages.push(ItlOverlayPage::Text(itl_quest_page_text(quest)));
    }
    for achievement in &submit_progress.achievements_completed {
        pages.push(ItlOverlayPage::Text(itl_achievement_page_text(achievement)));
    }
    pages.push(ItlOverlayPage::Leaderboard(leaderboard_entries_from_api(
        submit_leaderboard.to_vec(),
    )));
    pages
}

fn handle_submit_event_unlocks(
    player: &GrooveStatsSubmitPlayerJob,
    event: &GrooveStatsSubmitApiEvent,
) {
    let cfg = crate::config::get();
    if !cfg.auto_download_unlocks {
        return;
    }
    let Some(progress) = event.progress.as_ref() else {
        return;
    };
    let event_name = event_name_or_unknown(event.name.as_str());
    let profile_name = if player.profile_name.trim().is_empty() {
        "NoName"
    } else {
        player.profile_name.trim()
    };

    for quest in &progress.quests_completed {
        let url = quest.song_download_url.trim();
        if url.is_empty() {
            continue;
        }
        let title = quest.title.trim();
        let (download_name, pack_name) = if cfg.separate_unlocks_by_player {
            (
                format!("[{event_name}] {title} - {profile_name}"),
                format!("{event_name} Unlocks - {profile_name}"),
            )
        } else {
            (
                format!("[{event_name}] {title}"),
                format!("{event_name} Unlocks"),
            )
        };
        downloads::queue_event_unlock_download(url, download_name.trim_end(), pack_name.as_str());
    }
}

fn handle_submit_player_unlocks(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) {
    if let Some(itl) = response.itl.as_ref()
        && let Some(profile_id) = player.profile_id.as_deref()
        && let Some(progress) = itl.progress.as_ref()
    {
        for quest in &progress.quests_completed {
            update_itl_unlock_folders(profile_id, quest.song_download_folders.as_slice());
        }
    }
    if let Some(rpg) = response.rpg.as_ref() {
        handle_submit_event_unlocks(player, rpg);
    }
    if let Some(itl) = response.itl.as_ref() {
        handle_submit_event_unlocks(player, itl);
    }
}

fn itl_clear_type_change(
    progress: Option<&GrooveStatsSubmitApiProgress>,
) -> (Option<u8>, Option<u8>) {
    let Some(progress) = progress else {
        return (None, None);
    };
    for improvement in &progress.stat_improvements {
        if improvement.gained == 0 || !improvement.name.eq_ignore_ascii_case("clearType") {
            continue;
        }
        let after = improvement.current.clamp(0, i32::from(u8::MAX)) as u8;
        let before = after.saturating_sub(improvement.gained.min(u32::from(u8::MAX)) as u8);
        return (Some(before), Some(after));
    }
    (None, None)
}

fn itl_progress_from_submit(
    player: &GrooveStatsSubmitPlayerJob,
    response: &GrooveStatsSubmitApiPlayer,
) -> Option<ItlEventProgress> {
    let itl = response.itl.as_ref()?;
    let score_hundredths = player.itl_score_hundredths?;
    let (clear_type_before, clear_type_after) = itl_clear_type_change(itl.progress.as_ref());
    let mut progress = ItlEventProgress {
        name: event_name_or_unknown(itl.name.as_str()).to_string(),
        is_doubles: itl.is_doubles,
        score_hundredths,
        score_delta_hundredths: itl.score_delta,
        current_points: itl.top_score_points,
        point_delta: itl_delta_i32(itl.top_score_points, itl.prev_top_score_points),
        current_ranking_points: itl.current_ranking_point_total,
        ranking_delta: itl_delta_i32(
            itl.current_ranking_point_total,
            itl.previous_ranking_point_total,
        ),
        current_song_points: itl.current_song_point_total,
        song_delta: itl_delta_i32(itl.current_song_point_total, itl.previous_song_point_total),
        current_ex_points: itl.current_ex_point_total,
        ex_delta: itl_delta_i32(itl.current_ex_point_total, itl.previous_ex_point_total),
        current_total_points: itl.current_point_total,
        total_delta: itl_delta_i32(itl.current_point_total, itl.previous_point_total),
        total_passes: itl.total_passes,
        clear_type_before,
        clear_type_after,
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages = itl_overlay_pages(
        &progress,
        itl.progress.as_ref(),
        itl.itl_leaderboard.as_slice(),
    );
    Some(progress)
}

#[inline(always)]
fn itl_score_from_entry(entry: &ItlHashEntry) -> CachedItlScore {
    CachedItlScore {
        ex_hundredths: entry.ex,
        clear_type: entry.clear_type,
        points: entry.points,
    }
}

fn itl_score_for_song(
    song: &crate::game::song::SongData,
    data: &ItlFileData,
) -> Option<CachedItlScore> {
    let song_dir = itl_song_dir(song)?;
    let chart_hash = data.path_map.get(song_dir.as_str())?;
    data.hash_map.get(chart_hash).map(itl_score_from_entry)
}

fn itl_song_dir(song: &crate::game::song::SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
}

fn itl_group_name(song: &crate::game::song::SongData) -> Option<String> {
    let song_cache = get_song_cache();
    for pack in song_cache.iter() {
        if pack
            .songs
            .iter()
            .any(|candidate| candidate.simfile_path == song.simfile_path)
        {
            return Some(pack.group_name.clone());
        }
    }
    None
}

#[inline(always)]
fn itl_group_name_matches(group_name: &str) -> bool {
    let group = group_name.to_ascii_lowercase();
    group.contains("itl online 2026") || group.contains("itl 2026")
}

fn itl_is_song(
    song: &crate::game::song::SongData,
    song_dir: Option<&str>,
    data: &ItlFileData,
) -> bool {
    let song_dir_known = song_dir.is_some_and(|dir| data.path_map.contains_key(dir));
    if song_dir_known {
        return true;
    }
    let Some(group_name) = itl_group_name(song) else {
        return false;
    };
    itl_group_name_matches(group_name.as_str())
}

fn itl_chart_no_cmod(song: &crate::game::song::SongData, prev: Option<&ItlHashEntry>) -> bool {
    prev.map_or_else(
        || {
            song.display_subtitle(false)
                .to_ascii_lowercase()
                .contains("no cmod")
        },
        |data| data.no_cmod,
    )
}

#[inline(always)]
fn itl_event_name(song: &crate::game::song::SongData) -> String {
    itl_group_name(song).unwrap_or_else(|| "ITL Online 2026".to_string())
}

#[inline(always)]
fn itl_steps_type(chart: &crate::game::chart::ChartData) -> &'static str {
    if chart.chart_type.to_ascii_lowercase().contains("double") {
        "double"
    } else {
        "single"
    }
}

#[inline(always)]
fn itl_rank_for_points(sorted_points: &[u32], points: u32) -> Option<u32> {
    sorted_points
        .iter()
        .position(|value| *value == points)
        .map(|idx| idx.saturating_add(1) as u32)
}

fn itl_rebuild_song_ranks(data: &mut ItlFileData) {
    let mut points: Vec<u32> = data.hash_map.values().map(|entry| entry.points).collect();
    points.sort_unstable_by(|a, b| b.cmp(a));

    let mut points_single = Vec::with_capacity(points.len());
    let mut points_double = Vec::with_capacity(points.len());
    let mut unknown_points = Vec::new();
    let mut plays_single = 0usize;
    let mut plays_double = 0usize;

    for entry in data.hash_map.values_mut() {
        entry.rank = itl_rank_for_points(points.as_slice(), entry.points);
        if entry.steps_type.eq_ignore_ascii_case("single") {
            points_single.push(entry.points);
            plays_single = plays_single.saturating_add(1);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            points_double.push(entry.points);
            plays_double = plays_double.saturating_add(1);
        } else {
            unknown_points.push(entry.points);
        }
    }

    if plays_single > plays_double {
        points_single.extend(unknown_points);
    } else {
        points_double.extend(unknown_points);
    }

    points_single.sort_unstable_by(|a, b| b.cmp(a));
    points_double.sort_unstable_by(|a, b| b.cmp(a));

    for entry in data.hash_map.values_mut() {
        if entry.steps_type.eq_ignore_ascii_case("single") {
            entry.rank = itl_rank_for_points(points_single.as_slice(), entry.points);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            entry.rank = itl_rank_for_points(points_double.as_slice(), entry.points);
        }
    }

    data.points = points;
    data.points_single = points_single;
    data.points_double = points_double;
}

fn itl_point_totals(data: &ItlFileData) -> ItlPointTotals {
    let ranking_points = data.points.iter().take(75).copied().sum();
    let mut song_points = 0u32;
    let mut ex_points = 0u32;
    let mut total_points = 0u32;
    for entry in data.hash_map.values() {
        song_points = song_points.saturating_add(entry.passing_points);
        ex_points = ex_points.saturating_add(entry.points.saturating_sub(entry.passing_points));
        total_points = total_points.saturating_add(entry.points);
    }
    ItlPointTotals {
        ranking_points,
        song_points,
        ex_points,
        total_points,
    }
}

#[inline(always)]
fn itl_delta_i32(current: u32, previous: u32) -> i32 {
    (i64::from(current) - i64::from(previous)).clamp(i64::from(i32::MIN), i64::from(i32::MAX))
        as i32
}

fn loaded_itl_chart_no_cmod_for_gameplay(
    gs: &gameplay::State,
    player_idx: usize,
    profile_id: &str,
) -> Option<bool> {
    let song_dir = itl_song_dir(gs.song.as_ref())?;
    let state = ITL_SCORE_CACHE.lock().unwrap();
    let data = state.loaded_profiles.get(profile_id)?;
    if !itl_is_song(gs.song.as_ref(), Some(song_dir.as_str()), data) {
        return Some(false);
    }
    let prev = data.hash_map.get(gs.charts[player_idx].short_hash.as_str());
    Some(itl_chart_no_cmod(gs.song.as_ref(), prev))
}

pub fn should_warn_cmod_for_itl_chart(gs: &gameplay::State, player_idx: usize) -> bool {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS)
        || gs.course_display_totals.is_some()
        || !matches!(
            gs.player_profiles[player_idx].scroll_speed,
            crate::game::scroll::ScrollSpeedSetting::CMod(_)
        )
    {
        return false;
    }

    let side = gameplay_side_for_player(gs, player_idx);
    if let Some(profile_id) = profile::active_local_profile_id_for_side(side)
        && let Some(no_cmod) =
            loaded_itl_chart_no_cmod_for_gameplay(gs, player_idx, profile_id.as_str())
    {
        return no_cmod;
    }

    let Some(group_name) = itl_group_name(gs.song.as_ref()) else {
        return false;
    };
    itl_group_name_matches(group_name.as_str()) && itl_chart_no_cmod(gs.song.as_ref(), None)
}

fn parse_itl_points(chart_name: &str) -> Option<(u32, u32)> {
    let mut nums = chart_name
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());
    Some((nums.next()?, nums.next()?))
}

pub fn itl_points_for_chart(
    chart: &crate::game::chart::ChartData,
    ex_hundredths: u32,
) -> Option<u32> {
    let (passing_points, max_scoring_points) = parse_itl_points(chart.chart_name.as_str())?;
    Some(itl_points_for_song(
        passing_points,
        max_scoring_points,
        f64::from(ex_hundredths) / 100.0,
    ))
}

fn itl_points_for_song(passing_points: u32, max_scoring_points: u32, ex_score: f64) -> u32 {
    let scalar = 40.0_f64;
    let curve = (scalar.powf(ex_score.max(0.0) / scalar) - 1.0)
        * (100.0 / (scalar.powf(100.0 / scalar) - 1.0));
    let percent = ((curve / 100.0) * 1_000_000.0).round() / 1_000_000.0;
    passing_points.saturating_add((f64::from(max_scoring_points) * percent).floor() as u32)
}

fn itl_judgments_better(cur: &ItlJudgments, prev: &ItlJudgments) -> bool {
    for (cur_value, prev_value) in [
        (cur.w0, prev.w0),
        (cur.w1, prev.w1),
        (cur.w2, prev.w2),
        (cur.w3, prev.w3),
        (cur.w4, prev.w4),
        (cur.w5, prev.w5),
        (cur.miss, prev.miss),
    ] {
        match cur_value.cmp(&prev_value) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    false
}

fn itl_clear_type(judgments: &ItlJudgments) -> u8 {
    if judgments.total_rolls.saturating_sub(judgments.rolls) > 0
        || judgments.total_holds.saturating_sub(judgments.holds) > 0
    {
        return 1;
    }

    let mut clear_type = 1;
    let mut taps = judgments
        .miss
        .saturating_add(judgments.w5)
        .saturating_add(judgments.w4);
    if taps == 0 {
        clear_type = 2;
    }
    taps = taps.saturating_add(judgments.w3);
    if taps == 0 {
        clear_type = 3;
    }
    taps = taps.saturating_add(judgments.w2);
    if taps == 0 {
        clear_type = 4;
    }
    taps = taps.saturating_add(judgments.w1);
    if taps == 0 {
        clear_type = 5;
    }
    clear_type
}

fn itl_judgments_from_gameplay(gs: &gameplay::State, player_idx: usize) -> ItlJudgments {
    let counts = groovestats_judgment_counts(gs, player_idx);
    ItlJudgments {
        w0: counts.fantastic_plus,
        w1: counts.fantastic,
        w2: counts.excellent,
        w3: counts.great,
        w4: counts.decent,
        w5: counts.way_off,
        miss: counts.miss,
        total_steps: counts.total_steps,
        holds: counts.holds_held,
        total_holds: counts.total_holds,
        mines: counts.mines_hit,
        total_mines: counts.total_mines,
        rolls: counts.rolls_held,
        total_rolls: counts.total_rolls,
    }
}

fn itl_eval_state(gs: &gameplay::State, player_idx: usize, data: &ItlFileData) -> ItlEvalState {
    let used_cmod = matches!(
        gs.player_profiles[player_idx].scroll_speed,
        crate::game::scroll::ScrollSpeedSetting::CMod(_)
    );
    let Some(song_dir) = itl_song_dir(gs.song.as_ref()) else {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    };
    if !itl_is_song(gs.song.as_ref(), Some(song_dir.as_str()), data) {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod,
            reason_lines: Vec::new(),
        };
    }

    let chart_hash = gs.charts[player_idx].short_hash.as_str();
    let prev = data.hash_map.get(chart_hash);
    let chart_no_cmod = itl_chart_no_cmod(gs.song.as_ref(), prev);
    let gs_valid = groovestats_eval_state_from_gameplay(gs, player_idx);
    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate
    } else {
        1.0
    };
    let remove_mask =
        profile::normalize_remove_mask(gs.player_profiles[player_idx].remove_active_mask);
    let mines_enabled = (remove_mask & (1u8 << 1)) == 0;
    let passed = !gs.players[player_idx].is_failing && gs.song_completed_naturally;

    let mut reason_lines = Vec::with_capacity(4);
    if !gs_valid.valid {
        if gs_valid.reason_lines.is_empty() {
            reason_lines.push("Score is not valid for GrooveStats.".to_string());
        } else {
            reason_lines.extend(gs_valid.reason_lines);
        }
    }
    if (rate - 1.0).abs() > 0.0001 {
        reason_lines.push("ITL requires 1.00x music rate.".to_string());
    }
    if !mines_enabled {
        reason_lines.push("ITL requires mines to be enabled.".to_string());
    }
    if !passed {
        reason_lines.push("ITL only saves passing scores.".to_string());
    }
    if chart_no_cmod && used_cmod {
        reason_lines.push("This ITL chart does not allow CMod.".to_string());
    }

    ItlEvalState {
        active: true,
        eligible: reason_lines.is_empty(),
        chart_no_cmod,
        used_cmod,
        reason_lines,
    }
}

pub fn itl_eval_state_from_gameplay(gs: &gameplay::State, player_idx: usize) -> ItlEvalState {
    if player_idx >= gs.num_players.min(gameplay::MAX_PLAYERS) {
        return ItlEvalState::default();
    }
    let side = gameplay_side_for_player(gs, player_idx);
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return ItlEvalState::default();
    };
    let data = read_itl_file(profile_id.as_str());
    itl_eval_state(gs, player_idx, &data)
}

fn groovestats_submit_invalid_reason(
    chart: &crate::game::chart::ChartData,
    song_has_lua: bool,
    profile: &Profile,
    music_rate: f32,
) -> Option<String> {
    if song_has_lua {
        return Some("simfile relies on lua".to_string());
    }
    groovestats_eval_state(chart, profile, music_rate, false, false)
        .reason_lines
        .into_iter()
        .next()
}

#[inline(always)]
fn groovestats_judgment_counts(
    gs: &gameplay::State,
    player_idx: usize,
) -> GrooveStatsJudgmentCounts {
    let player = &gs.players[player_idx];
    let windows = gs.live_window_counts[player_idx];
    GrooveStatsJudgmentCounts {
        fantastic_plus: windows.w0,
        fantastic: windows.w1,
        excellent: windows.w2,
        great: windows.w3,
        decent: windows.w4,
        way_off: windows.w5,
        miss: windows.miss,
        total_steps: gs.total_steps[player_idx],
        holds_held: player.holds_held,
        total_holds: gs.holds_total[player_idx],
        mines_hit: player.mines_hit,
        total_mines: gs.mines_total[player_idx],
        rolls_held: player.rolls_held,
        total_rolls: gs.rolls_total[player_idx],
    }
}

#[inline(always)]
fn groovestats_rescore_add_target(counts: &mut GrooveStatsRescoreCounts, j: &judgment::Judgment) {
    if matches!(j.window, Some(judgment::TimingWindow::W0)) {
        counts.fantastic_plus = counts.fantastic_plus.saturating_add(1);
        return;
    }
    match j.grade {
        judgment::JudgeGrade::Fantastic => counts.fantastic = counts.fantastic.saturating_add(1),
        judgment::JudgeGrade::Excellent => counts.excellent = counts.excellent.saturating_add(1),
        judgment::JudgeGrade::Great => counts.great = counts.great.saturating_add(1),
        judgment::JudgeGrade::Decent => counts.decent = counts.decent.saturating_add(1),
        judgment::JudgeGrade::WayOff => counts.way_off = counts.way_off.saturating_add(1),
        judgment::JudgeGrade::Miss => {}
    }
}

fn groovestats_rescore_counts(gs: &gameplay::State, player_idx: usize) -> GrooveStatsRescoreCounts {
    let (start, end) = gs.note_ranges[player_idx];
    let mut counts = GrooveStatsRescoreCounts::default();
    for note in &gs.notes[start..end] {
        let Some(final_result) = note.result.as_ref() else {
            continue;
        };
        let Some(early_result) = note.early_result.as_ref() else {
            continue;
        };
        groovestats_rescore_add_target(&mut counts, final_result);
        groovestats_rescore_add_target(&mut counts, early_result);
    }
    counts
}

fn groovestats_comment_string(gs: &gameplay::State, player_idx: usize) -> String {
    let profile = &gs.player_profiles[player_idx];
    let counts = groovestats_judgment_counts(gs, player_idx);
    let mut parts: Vec<String> = Vec::with_capacity(10);

    if profile.show_fa_plus_window {
        let (start, end) = gs.note_ranges[player_idx];
        let ex = judgment::calculate_ex_score_from_notes(
            &gs.notes[start..end],
            &gs.note_time_cache[start..end],
            &gs.hold_end_time_cache[start..end],
            gs.total_steps[player_idx],
            gs.holds_total[player_idx],
            gs.rolls_total[player_idx],
            gs.mines_total[player_idx],
            gs.players[player_idx].fail_time,
            false,
        );
        parts.push("FA+".to_string());
        parts.push(format!("{ex:.2}EX"));
    }

    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate
    } else {
        1.0
    };
    if (rate - 1.0).abs() > 0.0001 {
        parts.push(format!("{}x Rate", compact_f32_text(rate)));
    }

    for (count, suffix) in [
        (counts.fantastic, "w"),
        (counts.excellent, "e"),
        (counts.great, "g"),
        (counts.decent, "d"),
        (counts.way_off, "wo"),
        (counts.miss, "m"),
    ] {
        if count != 0 {
            parts.push(format!("{count}{suffix}"));
        }
    }

    if let crate::game::scroll::ScrollSpeedSetting::CMod(value) = profile.scroll_speed {
        parts.push(format!("C{}", compact_f32_text(value)));
    }

    if parts.is_empty() {
        GROOVESTATS_COMMENT_PREFIX.to_string()
    } else {
        format!("{GROOVESTATS_COMMENT_PREFIX}, {}", parts.join(", "))
    }
}

fn groovestats_payload_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<GrooveStatsSubmitPlayerPayload> {
    if player_idx >= gs.num_players {
        return None;
    }
    let score_percent = judgment::calculate_itg_score_percent_from_counts(
        &gs.players[player_idx].scoring_counts,
        gs.players[player_idx].holds_held_for_score,
        gs.players[player_idx].rolls_held_for_score,
        gs.players[player_idx].mines_hit_for_score,
        gs.possible_grade_points[player_idx],
    );
    let score = (score_percent * 10000.0).round().clamp(0.0, 10000.0) as u32;
    let rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        (gs.music_rate * 100.0).round().clamp(0.0, u32::MAX as f32) as u32
    } else {
        100
    };

    Some(GrooveStatsSubmitPlayerPayload {
        rate,
        score,
        judgment_counts: groovestats_judgment_counts(gs, player_idx),
        rescore_counts: groovestats_rescore_counts(gs, player_idx),
        used_cmod: matches!(
            gs.player_profiles[player_idx].scroll_speed,
            crate::game::scroll::ScrollSpeedSetting::CMod(_)
        ),
        comment: groovestats_comment_string(gs, player_idx),
    })
}

fn log_body_snippet(text: &str) -> String {
    const MAX_LOG_CHARS: usize = 256;
    if text.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(text.len().min(MAX_LOG_CHARS));
    for ch in text.chars().take(MAX_LOG_CHARS) {
        out.push(ch);
    }
    out
}

fn submit_groovestats_request(
    job: &GrooveStatsSubmitRequest,
) -> Result<GrooveStatsSubmitApiResponse, GrooveStatsSubmitError> {
    let service_name = network::groovestats_service_name();
    let mut request = network::get_agent()
        .post(&groovestats_submit_url())
        .header("Content-Type", "application/json");
    for (name, value) in &job.headers {
        request = request.header(name, value);
    }
    for (name, value) in &job.query {
        request = request.query(name, value);
    }

    let response = request.send_json(&job.body).map_err(|e| {
        let message = format!("network error: {e}");
        let lower = message.to_ascii_lowercase();
        GrooveStatsSubmitError {
            status: if lower.contains("timeout") || lower.contains("timed out") {
                GrooveStatsSubmitUiStatus::TimedOut
            } else {
                GrooveStatsSubmitUiStatus::SubmitFailed
            },
            message,
        }
    })?;

    let status = response.status();
    let status_code = status.as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();
    if !status.is_success() {
        let snippet = log_body_snippet(body.as_str());
        let status_kind = if status_code == 408 || status_code == 504 {
            GrooveStatsSubmitUiStatus::TimedOut
        } else {
            GrooveStatsSubmitUiStatus::SubmitFailed
        };
        return Err(GrooveStatsSubmitError {
            status: status_kind,
            message: if snippet.is_empty() {
                format!("{service_name} submit returned HTTP {status_code}")
            } else {
                format!("{service_name} submit returned HTTP {status_code}: {snippet}")
            },
        });
    }

    let decoded: GrooveStatsSubmitApiResponse =
        serde_json::from_str(body.as_str()).map_err(|error| GrooveStatsSubmitError {
            status: GrooveStatsSubmitUiStatus::SubmitFailed,
            message: format!(
                "failed to parse {service_name} submit response: {}",
                log_body_snippet(error.to_string().as_str())
            ),
        })?;
    if !decoded.error.trim().is_empty() {
        return Err(GrooveStatsSubmitError {
            status: GrooveStatsSubmitUiStatus::SubmitFailed,
            message: format!("{service_name} submit error: {}", decoded.error.trim()),
        });
    }

    let snippet = log_body_snippet(body.as_str());
    if !snippet.is_empty() {
        debug!("{service_name} submit success body='{}'", snippet.as_str());
    } else {
        debug!("{service_name} submit success");
    }
    Ok(decoded)
}

pub fn submit_groovestats_payloads_from_gameplay(gs: &gameplay::State) {
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        groovestats_reset_submit_ui_status(side, gs.charts[player_idx].short_hash.as_str());
        groovestats_reset_submit_event_ui(side, gs.charts[player_idx].short_hash.as_str());
    }

    let cfg = crate::config::get();
    if !cfg.enable_groovestats || gs.num_players == 0 {
        return;
    }
    if gs.autoplay_used {
        debug!(
            "Skipping {} submit: autoplay/replay was used.",
            network::groovestats_service_name()
        );
        return;
    }
    if gs.course_display_totals.is_some() {
        debug!(
            "Skipping {} submit: course mode is unsupported by the old submit API.",
            network::groovestats_service_name()
        );
        return;
    }
    if gs.song.has_lua {
        debug!(
            "Skipping {} submit: simfile relies on lua.",
            network::groovestats_service_name()
        );
        return;
    }

    let network::ConnectionStatus::Connected(services) = network::get_status() else {
        debug!(
            "Skipping {} submit: service connection is not ready.",
            network::groovestats_service_name()
        );
        return;
    };
    if !services.auto_submit {
        debug!(
            "Skipping {} submit: auto-submit is not enabled by the service.",
            network::groovestats_service_name()
        );
        return;
    }

    let mut body = JsonMap::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    let mut headers = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    let mut query = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS) + 1);
    let mut players = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    query.push((
        "maxLeaderboardResults".to_string(),
        GROOVESTATS_SUBMIT_MAX_ENTRIES.to_string(),
    ));

    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let slot = if side == profile::PlayerSide::P1 {
            1
        } else {
            2
        };
        let profile = &gs.player_profiles[player_idx];
        let chart = gs.charts[player_idx].as_ref();

        if let Some(reason) =
            groovestats_submit_invalid_reason(chart, gs.song.has_lua, profile, gs.music_rate)
        {
            debug!(
                "Skipping {} submit for {:?} ({}): {}.",
                network::groovestats_service_name(),
                side,
                chart.short_hash,
                reason
            );
            continue;
        }
        if !profile.groovestats_is_pad_player {
            debug!(
                "Skipping {} submit for {:?} ({}): profile is not marked as a pad player.",
                network::groovestats_service_name(),
                side,
                chart.short_hash
            );
            continue;
        }
        if profile.groovestats_api_key.trim().is_empty() {
            continue;
        }
        let passed = !gs.players[player_idx].is_failing && gs.song_completed_naturally;
        if !passed {
            debug!(
                "Skipping {} submit for {:?} ({}): song was not passed.",
                network::groovestats_service_name(),
                side,
                chart.short_hash
            );
            continue;
        }

        let Some(payload) = groovestats_payload_for_player(gs, player_idx) else {
            continue;
        };
        let token = groovestats_next_submit_ui_token();
        groovestats_set_submit_ui_status(
            side,
            chart.short_hash.as_str(),
            token,
            GrooveStatsSubmitUiStatus::Submitting,
        );
        groovestats_arm_submit_event_ui(side, chart.short_hash.as_str(), token);
        players.push(GrooveStatsSubmitPlayerJob {
            side,
            slot,
            chart_hash: chart.short_hash.clone(),
            username: profile.groovestats_username.trim().to_string(),
            profile_name: profile.display_name.clone(),
            profile_id: profile::active_local_profile_id_for_side(side),
            token,
            itl_score_hundredths: Some(current_itl_score_hundredths(gs, player_idx)),
        });
        headers.push((
            format!("x-api-key-player-{slot}"),
            profile.groovestats_api_key.trim().to_string(),
        ));
        query.push((format!("chartHashP{slot}"), chart.short_hash.clone()));
        body.insert(
            format!("player{slot}"),
            serde_json::to_value(payload).expect("serialize GrooveStats submit payload"),
        );
    }

    if players.is_empty() {
        return;
    }

    let job = GrooveStatsSubmitRequest {
        players,
        headers,
        query,
        body: JsonValue::Object(body),
    };
    std::thread::spawn(move || match submit_groovestats_request(&job) {
        Ok(response) => {
            for player in &job.players {
                let Some(player_response) = response.player_for_slot(player.slot) else {
                    groovestats_update_submit_ui_status_if_token(
                        player.side,
                        player.chart_hash.as_str(),
                        player.token,
                        GrooveStatsSubmitUiStatus::SubmitFailed,
                    );
                    warn!(
                        "{} submit response omitted player{} for {:?} ({}).",
                        network::groovestats_service_name(),
                        player.slot,
                        player.side,
                        player.chart_hash
                    );
                    continue;
                };
                if !player_response.chart_hash.trim().is_empty()
                    && !player_response
                        .chart_hash
                        .eq_ignore_ascii_case(player.chart_hash.as_str())
                {
                    groovestats_update_submit_ui_status_if_token(
                        player.side,
                        player.chart_hash.as_str(),
                        player.token,
                        GrooveStatsSubmitUiStatus::SubmitFailed,
                    );
                    warn!(
                        "{} submit response hash mismatch for {:?}: expected {}, got {}.",
                        network::groovestats_service_name(),
                        player.side,
                        player.chart_hash,
                        player_response.chart_hash
                    );
                    continue;
                }

                groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    GrooveStatsSubmitUiStatus::Submitted,
                );
                groovestats_update_submit_event_ui_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    itl_progress_from_submit(player, player_response),
                );
                if let Some(profile_id) = player.profile_id.as_deref()
                    && !player.username.is_empty()
                    && !player_response.gs_leaderboard.is_empty()
                {
                    cache_gs_score_from_leaderboard(
                        profile_id,
                        player.username.as_str(),
                        player.chart_hash.as_str(),
                        player_response.gs_leaderboard.as_slice(),
                    );
                }
                handle_submit_player_unlocks(player, player_response);
                debug!(
                    "{} submit succeeded for {:?} ({}) result='{}'",
                    network::groovestats_service_name(),
                    player.side,
                    player.chart_hash,
                    player_response.result
                );
            }
        }
        Err(err) => {
            for player in &job.players {
                groovestats_update_submit_ui_status_if_token(
                    player.side,
                    player.chart_hash.as_str(),
                    player.token,
                    err.status,
                );
            }
            warn!("{}", err.message);
        }
    });
}

const ARROWCLOUD_BODY_VERSION: &str = "1.4";
const ARROWCLOUD_ENGINE_NAME: &str = "DeadSync";
const ARROWCLOUD_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
const ARROWCLOUD_SUBMIT_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_LIFEBAR_POINTS: usize = 100;
const ARROWCLOUD_ACCEL_NAMES: [&str; 5] = ["Boost", "Brake", "Wave", "Expand", "Boomerang"];
const ARROWCLOUD_EFFECT_NAMES: [&str; 10] = [
    "Drunk",
    "Dizzy",
    "Confusion",
    "Big",
    "Flip",
    "Invert",
    "Tornado",
    "Tipsy",
    "Bumpy",
    "Beat",
];
const ARROWCLOUD_APPEARANCE_NAMES: [&str; 5] = ["Hidden", "Sudden", "Stealth", "Blink", "R.Vanish"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudSubmitUiStatus {
    Submitting,
    Submitted,
    SubmitFailed,
    TimedOut,
}

#[derive(Debug, Clone)]
struct ArrowCloudSubmitUiEntry {
    chart_hash: String,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
}

static ARROWCLOUD_SUBMIT_UI_STATUS: std::sync::LazyLock<
    Mutex<[Option<ArrowCloudSubmitUiEntry>; 2]>,
> = std::sync::LazyLock::new(|| Mutex::new(std::array::from_fn(|_| None)));
static ARROWCLOUD_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[inline(always)]
const fn arrowcloud_side_ix(side: profile::PlayerSide) -> usize {
    match side {
        profile::PlayerSide::P1 => 0,
        profile::PlayerSide::P2 => 1,
    }
}

#[inline(always)]
fn arrowcloud_reset_submit_ui_status(side: profile::PlayerSide, chart_hash: &str) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let slot = &mut state[arrowcloud_side_ix(side)];
    if slot
        .as_ref()
        .is_some_and(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
    {
        *slot = None;
    }
}

#[inline(always)]
fn arrowcloud_set_submit_ui_status(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return;
    }
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    state[arrowcloud_side_ix(side)] = Some(ArrowCloudSubmitUiEntry {
        chart_hash: hash.to_string(),
        token,
        status,
    });
}

#[inline(always)]
fn arrowcloud_update_submit_ui_status_if_token(
    side: profile::PlayerSide,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    let mut state = ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap();
    let Some(entry) = state[arrowcloud_side_ix(side)].as_mut() else {
        return;
    };
    if entry.token != token || !entry.chart_hash.eq_ignore_ascii_case(chart_hash) {
        return;
    }
    entry.status = status;
}

#[inline(always)]
fn arrowcloud_next_submit_ui_token() -> u64 {
    ARROWCLOUD_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

pub fn get_arrowcloud_submit_ui_status_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<ArrowCloudSubmitUiStatus> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    ARROWCLOUD_SUBMIT_UI_STATUS.lock().unwrap()[arrowcloud_side_ix(side)]
        .as_ref()
        .filter(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        .map(|entry| entry.status)
}

#[derive(Debug, Serialize)]
struct ArrowCloudSpeed {
    value: f64,
    #[serde(rename = "type")]
    speed_type: &'static str,
}

#[derive(Debug, Serialize)]
struct ArrowCloudModifiers {
    #[serde(rename = "visualDelay")]
    visual_delay: i32,
    acceleration: Vec<String>,
    appearance: Vec<String>,
    effect: Vec<String>,
    mini: i32,
    turn: String,
    #[serde(rename = "disabledWindows")]
    disabled_windows: String,
    speed: ArrowCloudSpeed,
    perspective: String,
    noteskin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scroll: Option<String>,
}

#[derive(Debug, Serialize)]
struct ArrowCloudRadar {
    #[serde(rename = "Holds")]
    holds: [u32; 2],
    #[serde(rename = "Mines")]
    mines: [u32; 2],
    #[serde(rename = "Rolls")]
    rolls: [u32; 2],
}

#[derive(Debug, Serialize)]
struct ArrowCloudLifePoint {
    x: f64,
    y: f64,
}

#[derive(Debug, Serialize)]
struct ArrowCloudNpsPoint {
    x: f64,
    y: f64,
    measure: u32,
    nps: f64,
}

#[derive(Debug, Serialize)]
struct ArrowCloudNpsInfo {
    #[serde(rename = "peakNPS")]
    peak_nps: f64,
    points: Vec<ArrowCloudNpsPoint>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum ArrowCloudTimingOffset {
    Seconds(f64),
    Miss(&'static str),
}

type ArrowCloudTimingDatum = (f64, ArrowCloudTimingOffset);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudJudgmentCounts {
    fantastic_plus: u32,
    fantastic: u32,
    excellent: u32,
    great: u32,
    decent: u32,
    way_off: u32,
    miss: u32,
    total_steps: u32,
    holds_held: u32,
    total_holds: u32,
    mines_hit: u32,
    total_mines: u32,
    rolls_held: u32,
    total_rolls: u32,
}

#[derive(Debug, Serialize)]
struct ArrowCloudPayload {
    #[serde(rename = "songName")]
    song_name: String,
    artist: String,
    pack: String,
    length: String,
    hash: String,
    #[serde(rename = "timingData")]
    timing_data: Vec<ArrowCloudTimingDatum>,
    difficulty: u32,
    stepartist: String,
    radar: ArrowCloudRadar,
    #[serde(rename = "judgmentCounts")]
    judgment_counts: ArrowCloudJudgmentCounts,
    #[serde(rename = "npsInfo")]
    nps_info: ArrowCloudNpsInfo,
    #[serde(rename = "lifebarInfo")]
    lifebar_info: Vec<ArrowCloudLifePoint>,
    modifiers: ArrowCloudModifiers,
    #[serde(rename = "musicRate")]
    music_rate: f64,
    #[serde(rename = "usedAutoplay")]
    used_autoplay: bool,
    passed: bool,
    #[serde(rename = "bodyVersion")]
    body_version: &'static str,
    #[serde(rename = "_arrowCloudBodyVersion")]
    arrow_cloud_body_version: &'static str,
    #[serde(rename = "_engineName")]
    engine_name: &'static str,
    #[serde(rename = "_engineVersion")]
    engine_version: &'static str,
}

#[derive(Debug)]
struct ArrowCloudSubmitJob {
    side: profile::PlayerSide,
    api_key: String,
    token: u64,
    payload: ArrowCloudPayload,
}

#[derive(Debug)]
struct ArrowCloudSubmitError {
    status: ArrowCloudSubmitUiStatus,
    message: String,
}

#[inline(always)]
fn gameplay_side_for_player(gs: &gameplay::State, player_idx: usize) -> profile::PlayerSide {
    if gs.num_players >= 2 {
        if player_idx == 0 {
            profile::PlayerSide::P1
        } else {
            profile::PlayerSide::P2
        }
    } else {
        profile::get_session_player_side()
    }
}

#[inline(always)]
fn arrowcloud_format_length(seconds: f32) -> String {
    if !seconds.is_finite() || seconds <= 0.0 {
        return "0:00".to_string();
    }
    let total = seconds.floor() as i64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            (total % 3600) / 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

#[inline(always)]
fn arrowcloud_mask_labels_u8(mask: u8, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u8 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn arrowcloud_mask_labels_u16(mask: u16, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        if (mask & (1u16 << i)) != 0 {
            out.push((*name).to_string());
        }
    }
    out
}

#[inline(always)]
fn arrowcloud_turn_label(turn: profile::TurnOption) -> &'static str {
    match turn {
        profile::TurnOption::None => "None",
        profile::TurnOption::Mirror => "Mirror",
        profile::TurnOption::Left => "Left",
        profile::TurnOption::Right => "Right",
        profile::TurnOption::LRMirror => "LR-Mirror",
        profile::TurnOption::UDMirror => "UD-Mirror",
        profile::TurnOption::Shuffle
        | profile::TurnOption::Blender
        | profile::TurnOption::Random => "Shuffle",
    }
}

#[inline(always)]
fn arrowcloud_scroll_label(scroll: profile::ScrollOption) -> Option<String> {
    if scroll.contains(profile::ScrollOption::Reverse) {
        Some("Reverse".to_string())
    } else if scroll.contains(profile::ScrollOption::Split) {
        Some("Split".to_string())
    } else if scroll.contains(profile::ScrollOption::Alternate) {
        Some("Alternate".to_string())
    } else if scroll.contains(profile::ScrollOption::Cross) {
        Some("Cross".to_string())
    } else if scroll.contains(profile::ScrollOption::Centered) {
        Some("Centered".to_string())
    } else {
        None
    }
}

#[inline(always)]
fn arrowcloud_speed_payload(speed: crate::game::scroll::ScrollSpeedSetting) -> ArrowCloudSpeed {
    match speed {
        crate::game::scroll::ScrollSpeedSetting::CMod(v) => ArrowCloudSpeed {
            value: v as f64,
            speed_type: "C",
        },
        crate::game::scroll::ScrollSpeedSetting::MMod(v) => ArrowCloudSpeed {
            value: v as f64,
            speed_type: "M",
        },
        crate::game::scroll::ScrollSpeedSetting::XMod(v) => ArrowCloudSpeed {
            value: ((v as f64) * 100.0).round() / 100.0,
            speed_type: "X",
        },
    }
}

#[inline(always)]
fn arrowcloud_modifiers(profile: &Profile) -> ArrowCloudModifiers {
    ArrowCloudModifiers {
        visual_delay: profile.visual_delay_ms,
        acceleration: arrowcloud_mask_labels_u8(
            profile::normalize_accel_effects_mask(profile.accel_effects_active_mask),
            &ARROWCLOUD_ACCEL_NAMES,
        ),
        appearance: arrowcloud_mask_labels_u8(
            profile::normalize_appearance_effects_mask(profile.appearance_effects_active_mask),
            &ARROWCLOUD_APPEARANCE_NAMES,
        ),
        effect: arrowcloud_mask_labels_u16(
            profile::normalize_visual_effects_mask(profile.visual_effects_active_mask),
            &ARROWCLOUD_EFFECT_NAMES,
        ),
        mini: profile.mini_percent.clamp(-100, 150),
        turn: arrowcloud_turn_label(profile.turn_option).to_string(),
        disabled_windows: "None".to_string(),
        speed: arrowcloud_speed_payload(profile.scroll_speed),
        perspective: profile.perspective.to_string(),
        noteskin: profile.noteskin.as_str().to_string(),
        scroll: arrowcloud_scroll_label(profile.scroll_option),
    }
}

#[inline(always)]
fn arrowcloud_life_lerp_at(life_history: &[(f32, f32)], sample_time: f32) -> f32 {
    let Some(&(_, first_life)) = life_history.first() else {
        return 0.0;
    };
    if life_history.len() == 1 {
        return first_life.clamp(0.0, 1.0);
    }

    let later_ix = life_history.partition_point(|&(t, _)| t <= sample_time);
    let earlier_ix = later_ix.saturating_sub(1).min(life_history.len() - 1);
    let (earlier_t, earlier_life) = life_history[earlier_ix];
    if later_ix >= life_history.len() {
        return earlier_life.clamp(0.0, 1.0);
    }

    let (later_t, later_life) = life_history[later_ix];
    let dt = later_t - earlier_t;
    if dt.abs() <= f32::EPSILON {
        return earlier_life.clamp(0.0, 1.0);
    }
    let alpha = ((sample_time - earlier_t) / dt).clamp(0.0, 1.0);
    (earlier_life + (later_life - earlier_life) * alpha).clamp(0.0, 1.0)
}

#[inline(always)]
fn arrowcloud_lifebar_points(gs: &gameplay::State, player_idx: usize) -> Vec<ArrowCloudLifePoint> {
    let life_history = gs.players[player_idx].life_history.as_slice();
    if life_history.is_empty() {
        return Vec::new();
    }
    let (start, end) = gs.note_ranges[player_idx];
    let note_times = &gs.note_time_cache[start..end];
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    let chart_start_second = note_times
        .iter()
        .copied()
        .find(|t| t.is_finite())
        .unwrap_or(first_second);
    let duration = (last_second - first_second).max(0.0);
    let step = duration / ARROWCLOUD_LIFEBAR_POINTS as f32;

    let mut out = Vec::with_capacity(ARROWCLOUD_LIFEBAR_POINTS);
    for i in 0..ARROWCLOUD_LIFEBAR_POINTS {
        let x = chart_start_second + (i as f32 * step);
        out.push(ArrowCloudLifePoint {
            x: x as f64,
            y: arrowcloud_life_lerp_at(life_history, x) as f64,
        });
    }
    out
}

#[inline(always)]
fn arrowcloud_timing_data_from_scatter(
    scatter: &[crate::game::timing::ScatterPoint],
) -> Vec<ArrowCloudTimingDatum> {
    let mut out = Vec::with_capacity(scatter.len());
    for point in scatter {
        if !point.time_sec.is_finite() {
            continue;
        }
        let value = if let Some(offset_ms) = point.offset_ms {
            if !offset_ms.is_finite() {
                continue;
            }
            ArrowCloudTimingOffset::Seconds((offset_ms / 1000.0) as f64)
        } else {
            ArrowCloudTimingOffset::Miss("Miss")
        };
        out.push((point.time_sec as f64, value));
    }
    out
}

#[inline(always)]
fn arrowcloud_timing_data(gs: &gameplay::State, player_idx: usize) -> Vec<ArrowCloudTimingDatum> {
    let (start, end) = gs.note_ranges[player_idx];
    let notes = &gs.notes[start..end];
    let note_times = &gs.note_time_cache[start..end];
    let col_offset = player_idx.saturating_mul(gs.cols_per_player);
    let stream_segments = gameplay::stream_segments_for_results(gs, player_idx);
    let scatter = crate::game::timing::build_scatter_points(
        notes,
        note_times,
        col_offset,
        gs.cols_per_player,
        &stream_segments,
    );
    arrowcloud_timing_data_from_scatter(&scatter)
}

#[inline(always)]
fn arrowcloud_nps_info(gs: &gameplay::State, player_idx: usize) -> ArrowCloudNpsInfo {
    let chart = gs.charts[player_idx].as_ref();
    let first_second = gs.density_graph_first_second.min(0.0);
    let last_second = gs.density_graph_last_second.max(first_second);
    let peak_nps = if chart.max_nps.is_finite() && chart.max_nps > 0.0 {
        chart.max_nps
    } else {
        0.0
    };

    let mut points = Vec::with_capacity(chart.measure_nps_vec.len());
    let mut started = false;
    for (measure, nps) in chart.measure_nps_vec.iter().copied().enumerate() {
        if !nps.is_finite() {
            continue;
        }
        if nps > 0.0 {
            started = true;
        }
        if !started {
            continue;
        }
        let Some(&t) = chart.measure_seconds_vec.get(measure) else {
            continue;
        };
        let x = if last_second > first_second {
            ((t - first_second) / (last_second - first_second)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let y = if peak_nps > 0.0 {
            (nps / peak_nps).clamp(0.0, 1.0)
        } else {
            0.0
        };
        points.push(ArrowCloudNpsPoint {
            x: x as f64,
            y: y as f64,
            measure: measure as u32,
            nps,
        });
    }

    ArrowCloudNpsInfo { peak_nps, points }
}

#[inline(always)]
fn arrowcloud_judgment_counts(gs: &gameplay::State, player_idx: usize) -> ArrowCloudJudgmentCounts {
    let player = &gs.players[player_idx];
    let counts = player.judgment_counts;
    let windows = gs.live_window_counts[player_idx];
    let fantastic_total = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)];
    let fantastic_plus = windows.w0;
    let fantastic = fantastic_total.saturating_sub(fantastic_plus);
    let excellent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)];
    let great = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)];
    let decent = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Decent)];
    let way_off = counts[judgment::judge_grade_ix(judgment::JudgeGrade::WayOff)];
    let miss = counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)];
    let mut total_steps = 0u32;
    for count in counts {
        total_steps = total_steps.saturating_add(count);
    }

    ArrowCloudJudgmentCounts {
        fantastic_plus,
        fantastic,
        excellent,
        great,
        decent,
        way_off,
        miss,
        total_steps,
        holds_held: player.holds_held,
        total_holds: gs.holds_total[player_idx],
        mines_hit: player.mines_hit,
        total_mines: gs.mines_total[player_idx],
        rolls_held: player.rolls_held,
        total_rolls: gs.rolls_total[player_idx],
    }
}

#[inline(always)]
fn arrowcloud_payload_for_player(
    gs: &gameplay::State,
    player_idx: usize,
) -> Option<ArrowCloudPayload> {
    if player_idx >= gs.num_players {
        return None;
    }
    let chart = gs.charts[player_idx].as_ref();
    let profile = &gs.player_profiles[player_idx];
    let player = &gs.players[player_idx];
    let pack = gs.pack_group.trim().to_string();
    let song_name = gs.song.display_full_title(true);
    let music_rate = if gs.music_rate.is_finite() && gs.music_rate > 0.0 {
        gs.music_rate as f64
    } else {
        1.0
    };
    let passed = !player.is_failing && gs.song_completed_naturally;

    Some(ArrowCloudPayload {
        song_name,
        artist: gs.song.artist.clone(),
        pack,
        length: arrowcloud_format_length(gs.song.music_length_seconds),
        hash: chart.short_hash.clone(),
        timing_data: arrowcloud_timing_data(gs, player_idx),
        difficulty: chart.meter,
        stepartist: chart.step_artist.clone(),
        radar: ArrowCloudRadar {
            holds: [player.holds_held, gs.holds_total[player_idx]],
            mines: [player.mines_avoided, gs.mines_total[player_idx]],
            rolls: [player.rolls_held, gs.rolls_total[player_idx]],
        },
        judgment_counts: arrowcloud_judgment_counts(gs, player_idx),
        nps_info: arrowcloud_nps_info(gs, player_idx),
        lifebar_info: arrowcloud_lifebar_points(gs, player_idx),
        modifiers: arrowcloud_modifiers(profile),
        music_rate,
        used_autoplay: gs.autoplay_used,
        passed,
        body_version: ARROWCLOUD_BODY_VERSION,
        arrow_cloud_body_version: ARROWCLOUD_BODY_VERSION,
        engine_name: ARROWCLOUD_ENGINE_NAME,
        engine_version: ARROWCLOUD_ENGINE_VERSION,
    })
}

#[inline(always)]
fn arrowcloud_submit_url(chart_hash: &str) -> Option<String> {
    let hash = chart_hash.trim();
    if hash.is_empty() {
        return None;
    }
    Some(format!(
        "{}/v1/chart/{hash}/play",
        ARROWCLOUD_SUBMIT_BASE_URL.trim_end_matches('/')
    ))
}

#[inline(always)]
fn submit_arrowcloud_payload(
    side: profile::PlayerSide,
    api_key: &str,
    payload: &ArrowCloudPayload,
) -> Result<(), ArrowCloudSubmitError> {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return Err(ArrowCloudSubmitError {
            status: ArrowCloudSubmitUiStatus::SubmitFailed,
            message: "missing ArrowCloud API key".to_string(),
        });
    }
    let Some(url) = arrowcloud_submit_url(payload.hash.as_str()) else {
        return Err(ArrowCloudSubmitError {
            status: ArrowCloudSubmitUiStatus::SubmitFailed,
            message: "missing chart hash".to_string(),
        });
    };

    let bearer = format!("Bearer {api_key}");
    let agent = network::get_agent();
    let response = agent
        .post(&url)
        .header("Content-Type", "application/json")
        .header("Authorization", &bearer)
        .send_json(payload)
        .map_err(|e| {
            let msg = format!("network error: {e}");
            let lower = msg.to_ascii_lowercase();
            ArrowCloudSubmitError {
                status: if lower.contains("timeout") || lower.contains("timed out") {
                    ArrowCloudSubmitUiStatus::TimedOut
                } else {
                    ArrowCloudSubmitUiStatus::SubmitFailed
                },
                message: msg,
            }
        })?;
    let status = response.status();
    let status_code = status.as_u16();
    let body = response.into_body().read_to_string().unwrap_or_default();
    if status.is_success() {
        let snippet = log_body_snippet(body.as_str());
        if !snippet.is_empty() {
            debug!(
                "ArrowCloud submit success for {:?} ({}) status={} body='{}'",
                side,
                payload.hash,
                status_code,
                snippet.as_str()
            );
        } else {
            debug!(
                "ArrowCloud submit success for {:?} ({}) status={}",
                side, payload.hash, status_code
            );
        }
        return Ok(());
    }

    let snippet = log_body_snippet(body.as_str());
    let status_kind = if status_code == 408 || status_code == 504 {
        ArrowCloudSubmitUiStatus::TimedOut
    } else {
        ArrowCloudSubmitUiStatus::SubmitFailed
    };
    if snippet.is_empty() {
        Err(ArrowCloudSubmitError {
            status: status_kind,
            message: format!("HTTP {status_code}"),
        })
    } else {
        Err(ArrowCloudSubmitError {
            status: status_kind,
            message: format!("HTTP {status_code}: {}", snippet.as_str()),
        })
    }
}

pub fn submit_arrowcloud_payloads_from_gameplay(gs: &gameplay::State) {
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let side = gameplay_side_for_player(gs, player_idx);
        let chart_hash = gs.charts[player_idx].short_hash.as_str();
        arrowcloud_reset_submit_ui_status(side, chart_hash);
    }

    let cfg = crate::config::get();
    if !cfg.enable_arrowcloud || gs.num_players == 0 {
        return;
    }
    if gs.autoplay_used {
        debug!("Skipping ArrowCloud submit: autoplay/replay was used.");
        return;
    }
    if gs.course_display_totals.is_some() && !cfg.autosubmit_course_scores_individually {
        debug!("Skipping ArrowCloud submit: course per-song autosubmit is disabled.");
        return;
    }
    if gs.song.has_lua {
        debug!("Skipping ArrowCloud submit: simfile relies on lua.");
        return;
    }
    if let network::ArrowCloudConnectionStatus::Error(msg) = network::get_arrowcloud_status() {
        warn!("Skipping ArrowCloud submit due to connection status error: {msg}");
        return;
    }

    let mut jobs = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        if !gs.score_valid[player_idx] {
            debug!(
                "Skipping ArrowCloud submit for player {}: ranking-invalid modifiers were used.",
                player_idx + 1
            );
            continue;
        }

        let side = gameplay_side_for_player(gs, player_idx);
        let api_key = gs.player_profiles[player_idx].arrowcloud_api_key.trim();
        if api_key.is_empty() {
            continue;
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx) else {
            continue;
        };
        if !payload.passed {
            debug!(
                "Skipping ArrowCloud submit for {:?} ({}) : song was not passed.",
                side, payload.hash
            );
            continue;
        }
        let token = arrowcloud_next_submit_ui_token();
        arrowcloud_set_submit_ui_status(
            side,
            payload.hash.as_str(),
            token,
            ArrowCloudSubmitUiStatus::Submitting,
        );
        jobs.push(ArrowCloudSubmitJob {
            side,
            api_key: api_key.to_string(),
            token,
            payload,
        });
    }
    if jobs.is_empty() {
        return;
    }

    std::thread::spawn(move || {
        for job in jobs {
            match submit_arrowcloud_payload(job.side, &job.api_key, &job.payload) {
                Ok(()) => arrowcloud_update_submit_ui_status_if_token(
                    job.side,
                    job.payload.hash.as_str(),
                    job.token,
                    ArrowCloudSubmitUiStatus::Submitted,
                ),
                Err(err) => {
                    arrowcloud_update_submit_ui_status_if_token(
                        job.side,
                        job.payload.hash.as_str(),
                        job.token,
                        err.status,
                    );
                    warn!(
                        "ArrowCloud submit failed for {:?} ({}) : {}",
                        job.side, job.payload.hash, err.message
                    );
                }
            }
        }
    });
}

pub fn save_local_summary_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    music_rate: f32,
    summary: &stage_stats::PlayerStageSummary,
) {
    if chart_hash.trim().is_empty() {
        return;
    }
    if !summary.score_valid {
        debug!("Skipping local summary score save: ranking-invalid modifiers were used.");
        return;
    }
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return;
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let profile_initials = profile::get_for_side(side).player_initials;
    let counts = [
        summary
            .window_counts
            .w0
            .saturating_add(summary.window_counts.w1),
        summary.window_counts.w2,
        summary.window_counts.w3,
        summary.window_counts.w4,
        summary.window_counts.w5,
        summary.window_counts.miss,
    ];
    let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, summary.grade);
    let mut entry = LocalScoreEntryV1 {
        version: LOCAL_SCORE_VERSION_V1,
        played_at_ms: now_ms,
        music_rate: if music_rate.is_finite() && music_rate > 0.0 {
            music_rate
        } else {
            1.0
        },
        score_percent: summary.score_percent.clamp(0.0, 1.0),
        grade_code: grade_to_code(summary.grade),
        lamp_index,
        lamp_judge_count,
        ex_score_percent: summary.ex_score_percent.clamp(0.0, 100.0),
        hard_ex_score_percent: summary.hard_ex_score_percent.clamp(0.0, 100.0),
        judgment_counts: counts,
        holds_held: 0,
        holds_total: 0,
        rolls_held: 0,
        rolls_total: 0,
        mines_avoided: 0,
        mines_total: 0,
        hands_achieved: 0,
        fail_time: (summary.grade == Grade::Failed).then_some(0.0),
        beat0_time_seconds: 0.0,
        replay: Vec::new(),
    };
    append_local_score_on_disk(
        &profile_id,
        profile_initials.as_str(),
        chart_hash,
        &mut entry,
    );
}

// --- API Response Structs ---

#[derive(Debug, Clone)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub name: String,
    pub machine_tag: Option<String>,
    pub score: f64, // 0..10000
    pub date: String,
    pub is_rival: bool,
    pub is_self: bool,
    pub is_fail: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ReplayEdge {
    pub event_music_time: f32,
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
}

#[derive(Debug, Clone)]
pub struct MachineReplayEntry {
    pub rank: u32,
    pub name: String,
    pub score: f64, // 0..10000
    pub date: String,
    pub is_fail: bool,
    pub replay_beat0_time_seconds: f32,
    pub replay: Vec<ReplayEdge>,
}

#[derive(Debug, Clone)]
pub struct LeaderboardPane {
    pub name: String,
    pub entries: Vec<LeaderboardEntry>,
    pub is_ex: bool,
    pub disabled: bool,
}

impl LeaderboardPane {
    #[inline(always)]
    pub fn is_groovestats(&self) -> bool {
        self.name.eq_ignore_ascii_case("GrooveStats")
    }

    #[inline(always)]
    pub fn is_arrowcloud(&self) -> bool {
        self.name.eq_ignore_ascii_case("ArrowCloud")
    }

    #[inline(always)]
    pub fn is_hard_ex(&self) -> bool {
        self.is_arrowcloud()
    }
}

#[derive(Debug, Clone)]
pub struct PlayerLeaderboardData {
    pub panes: Vec<LeaderboardPane>,
    pub itl_self_score: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CachedPlayerLeaderboardData {
    pub loading: bool,
    pub data: Option<PlayerLeaderboardData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PlayerLeaderboardCacheKey {
    chart_hash: String,
    api_key: String,
    arrowcloud_api_key: String,
    include_arrowcloud: bool,
    show_ex_score: bool,
}

#[derive(Debug, Clone)]
enum PlayerLeaderboardCacheValue {
    Ready(PlayerLeaderboardData),
    Error(String),
}

#[derive(Debug, Clone)]
struct PlayerLeaderboardCacheEntry {
    value: PlayerLeaderboardCacheValue,
    max_entries: usize,
    refreshed_at: Instant,
    retry_after: Option<Instant>,
}

#[derive(Default)]
struct PlayerLeaderboardCacheState {
    by_key: HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    in_flight: HashSet<PlayerLeaderboardCacheKey>,
}

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);
const ARROWCLOUD_LEADERBOARDS_BASE_URL: &str = "https://api.arrowcloud.dance";
const ARROWCLOUD_HARD_EX_MIN_PER_PAGE: usize = 16;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardsApiResponse {
    player1: Option<LeaderboardApiPlayer>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardsApiResponse {
    #[serde(default)]
    leaderboards: Vec<ArrowCloudLeaderboardPane>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardPane {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    scores: Vec<ArrowCloudLeaderboardEntry>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ArrowCloudLeaderboardEntry {
    #[serde(default, deserialize_with = "de_u32_from_string_or_number")]
    rank: u32,
    #[serde(default, deserialize_with = "de_f64_from_string_or_number")]
    score: f64, // 0..100
    #[serde(default)]
    alias: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    is_rival: bool,
    #[serde(default)]
    is_self: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum U32OrString {
    U32(u32),
    F64(f64),
    String(String),
}

fn de_u32_from_string_or_number<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<U32OrString>::deserialize(deserializer)? {
        Some(U32OrString::U32(v)) => Ok(v),
        Some(U32OrString::F64(v)) => Ok(v.max(0.0).floor() as u32),
        Some(U32OrString::String(text)) => Ok(text.trim().parse::<u32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum I32OrString {
    I32(i32),
    I64(i64),
    F64(f64),
    String(String),
}

fn de_i32_from_string_or_number<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<I32OrString>::deserialize(deserializer)? {
        Some(I32OrString::I32(v)) => Ok(v),
        Some(I32OrString::I64(v)) => Ok(v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        Some(I32OrString::F64(v)) => {
            Ok(v.clamp(f64::from(i32::MIN), f64::from(i32::MAX)).round() as i32)
        }
        Some(I32OrString::String(text)) => Ok(text.trim().parse::<i32>().unwrap_or(0)),
        None => Ok(0),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum F64OrString {
    F64(f64),
    String(String),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum StringOrNumber {
    String(String),
    I64(i64),
    U64(u64),
    F64(f64),
}

fn de_f64_from_string_or_number<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<F64OrString>::deserialize(deserializer)? {
        Some(F64OrString::F64(v)) => Ok(v),
        Some(F64OrString::String(text)) => Ok(text.trim().parse::<f64>().unwrap_or(0.0)),
        None => Ok(0.0),
    }
}

fn de_string_from_string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<StringOrNumber>::deserialize(deserializer)? {
        Some(StringOrNumber::String(text)) => Ok(text),
        Some(StringOrNumber::I64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::U64(v)) => Ok(v.to_string()),
        Some(StringOrNumber::F64(v)) => Ok(compact_f32_text(v as f32)),
        None => Ok(String::new()),
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardApiPlayer {
    #[serde(default)]
    is_ranked: bool,
    #[serde(rename = "gsLeaderboard", default)]
    gs_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "exLeaderboard", default)]
    ex_leaderboard: Vec<LeaderboardApiEntry>,
    rpg: Option<LeaderboardEventData>,
    itl: Option<LeaderboardEventData>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardEventData {
    #[serde(default)]
    name: String,
    #[serde(rename = "rpgLeaderboard", default)]
    rpg_leaderboard: Vec<LeaderboardApiEntry>,
    #[serde(rename = "itlLeaderboard", default)]
    itl_leaderboard: Vec<LeaderboardApiEntry>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
struct LeaderboardApiEntry {
    #[serde(default)]
    rank: u32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    machine_tag: Option<String>,
    #[serde(default)]
    score: f64, // 0..10000
    #[serde(default)]
    date: String,
    #[serde(default)]
    is_rival: bool,
    #[serde(default)]
    is_self: bool,
    #[serde(default)]
    is_fail: bool,
    #[serde(default)]
    comments: Option<String>,
}

fn leaderboard_entries_from_api(entries: Vec<LeaderboardApiEntry>) -> Vec<LeaderboardEntry> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        out.push(LeaderboardEntry {
            rank: entry.rank,
            name: entry.name,
            machine_tag: entry.machine_tag,
            score: entry.score,
            date: entry.date,
            is_rival: entry.is_rival,
            is_self: entry.is_self,
            is_fail: entry.is_fail,
        });
    }
    out
}

fn push_leaderboard_pane(
    out: &mut Vec<LeaderboardPane>,
    name: &str,
    entries: Vec<LeaderboardApiEntry>,
    is_ex: bool,
) {
    if entries.is_empty() {
        return;
    }
    out.push(LeaderboardPane {
        name: name.to_string(),
        entries: leaderboard_entries_from_api(entries),
        is_ex,
        disabled: false,
    });
}

struct FetchedPlayerLeaderboards {
    data: PlayerLeaderboardData,
    gs_entries: Vec<LeaderboardApiEntry>,
}

#[inline(always)]
fn leaderboard_self_score_10000(entries: &[LeaderboardApiEntry], username: &str) -> Option<u32> {
    let entry = entries.iter().find(|entry| entry.is_self).or_else(|| {
        (!username.trim().is_empty()).then(|| {
            entries
                .iter()
                .find(|entry| entry.name.eq_ignore_ascii_case(username))
        })?
    })?;
    if entry.is_fail || !entry.score.is_finite() {
        return None;
    }
    Some(entry.score.round().clamp(0.0, 10000.0) as u32)
}

#[inline(always)]
const fn cached_failed_gs_score() -> CachedScore {
    CachedScore {
        grade: Grade::Failed,
        score_percent: 0.0,
        lamp_index: None,
        lamp_judge_count: None,
    }
}

#[inline(always)]
fn cached_score_from_gs(
    score_10000: f64,
    comments: Option<&str>,
    chart_hash: &str,
    is_fail: bool,
) -> CachedScore {
    if is_fail {
        return cached_failed_gs_score();
    }
    let lamp_index = compute_lamp_index(score_10000, comments, chart_hash);
    let lamp_judge_count = compute_lamp_judge_count(lamp_index, comments);
    CachedScore {
        grade: score_to_grade(score_10000),
        score_percent: score_10000 / 10000.0,
        lamp_index,
        lamp_judge_count,
    }
}

fn cache_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
) {
    set_cached_gs_score_for_profile(profile_id, chart_hash.to_string(), score);
    if !username.trim().is_empty() {
        append_gs_score_on_disk_for_profile(profile_id, chart_hash, score, username);
    }
}

fn cache_gs_score_from_leaderboard(
    profile_id: &str,
    username: &str,
    chart_hash: &str,
    gs_entries: &[LeaderboardApiEntry],
) {
    let self_entry = gs_entries.iter().find(|entry| entry.is_self).or_else(|| {
        gs_entries
            .iter()
            .find(|entry| entry.name.eq_ignore_ascii_case(username))
    });
    let Some(entry) = self_entry else {
        set_cached_gs_score_for_profile(
            profile_id,
            chart_hash.to_string(),
            cached_failed_gs_score(),
        );
        return;
    };
    let score = cached_score_from_gs(
        entry.score,
        entry.comments.as_deref(),
        chart_hash,
        entry.is_fail,
    );
    cache_gs_score_for_profile(profile_id, chart_hash, score, username);
}

#[inline(always)]
fn arrowcloud_lb_type_is_hard_ex(lb_type: &str) -> bool {
    if lb_type.is_empty() {
        return false;
    }
    let mut compact = String::with_capacity(lb_type.len());
    for ch in lb_type.chars() {
        if ch.is_ascii_alphanumeric() {
            compact.push(ch.to_ascii_lowercase());
        }
    }
    compact == "hardex" || compact == "hex"
}

fn arrowcloud_entries_from_api(entries: Vec<ArrowCloudLeaderboardEntry>) -> Vec<LeaderboardEntry> {
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        let score = if entry.score.is_finite() {
            (entry.score * 100.0).clamp(0.0, 10000.0)
        } else {
            0.0
        };
        out.push(LeaderboardEntry {
            rank: entry.rank,
            name: entry.alias,
            machine_tag: None,
            score,
            date: entry.date,
            is_rival: entry.is_rival,
            is_self: entry.is_self,
            is_fail: false,
        });
    }
    out
}

fn fetch_arrowcloud_hard_ex_pane(
    chart_hash: &str,
    api_key: &str,
    max_entries: usize,
) -> Result<Option<LeaderboardPane>, Box<dyn Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return Ok(None);
    }

    // ArrowCloud may return self/rival entries outside the top ranks.
    // Pull a wider page so scorebox views can always include those rows.
    let max_entries = max_entries.max(1).max(ARROWCLOUD_HARD_EX_MIN_PER_PAGE);
    let max_entries = max_entries.to_string();
    let api_url = format!(
        "{}/v1/chart/{chart_hash}/leaderboards",
        ARROWCLOUD_LEADERBOARDS_BASE_URL.trim_end_matches('/')
    );
    let bearer = format!("Bearer {api_key}");
    let response = network::get_agent()
        .get(&api_url)
        .header("Authorization", &bearer)
        .header("x-api-key-player-1", api_key)
        .query("page", "1")
        .query("perPage", max_entries.as_str())
        .call()?;

    if response.status() != 200 {
        return Err(format!(
            "ArrowCloud leaderboard API returned status {}",
            response.status()
        )
        .into());
    }

    let decoded: ArrowCloudLeaderboardsApiResponse = response.into_body().read_json()?;
    let hard_ex = decoded
        .leaderboards
        .into_iter()
        .find(|pane| arrowcloud_lb_type_is_hard_ex(pane.r#type.as_str()));
    let Some(hard_ex) = hard_ex else {
        return Ok(None);
    };
    if hard_ex.scores.is_empty() {
        return Ok(None);
    }
    Ok(Some(LeaderboardPane {
        name: "ArrowCloud".to_string(),
        entries: arrowcloud_entries_from_api(hard_ex.scores),
        is_ex: false,
        disabled: false,
    }))
}

fn fetch_player_leaderboards_internal(
    chart_hash: &str,
    api_key: &str,
    username: &str,
    arrowcloud_api_key: Option<&str>,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<FetchedPlayerLeaderboards, Box<dyn Error + Send + Sync>> {
    if chart_hash.trim().is_empty() {
        return Err("Missing chart hash for leaderboard request.".into());
    }
    if api_key.trim().is_empty() {
        return Err("Missing GrooveStats API key for leaderboard request.".into());
    }

    let max_entries = max_entries.max(1);
    let max_entries_str = max_entries.to_string();
    let agent = network::get_agent();
    let api_url = network::groovestats_player_leaderboards_url();
    let response = agent
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash)
        .query("maxLeaderboardResults", &max_entries_str)
        .call()?;

    if response.status() != 200 {
        return Err(format!("Leaderboard API returned status {}", response.status()).into());
    }

    let decoded: LeaderboardsApiResponse = response.into_body().read_json()?;
    let mut panes = Vec::with_capacity(5);
    let mut gs_entries = Vec::new();
    let mut itl_self_score = None;
    if let Some(player) = decoded.player1 {
        let LeaderboardApiPlayer {
            is_ranked: _is_ranked,
            gs_leaderboard,
            ex_leaderboard,
            rpg,
            itl,
        } = player;

        gs_entries = gs_leaderboard.clone();
        if show_ex_score {
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
        } else {
            push_leaderboard_pane(&mut panes, "GrooveStats", gs_leaderboard, false);
            push_leaderboard_pane(&mut panes, "GrooveStats", ex_leaderboard, true);
        }

        if let Some(rpg) = rpg
            && !rpg.rpg_leaderboard.is_empty()
        {
            let name = if rpg.name.trim().is_empty() {
                "RPG"
            } else {
                rpg.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, rpg.rpg_leaderboard, false);
        }
        if let Some(itl) = itl
            && !itl.itl_leaderboard.is_empty()
        {
            itl_self_score = leaderboard_self_score_10000(&itl.itl_leaderboard, username);
            let name = if itl.name.trim().is_empty() {
                "ITL"
            } else {
                itl.name.as_str()
            };
            push_leaderboard_pane(&mut panes, name, itl.itl_leaderboard, true);
        }
    }

    if let Some(arrowcloud_api_key) = arrowcloud_api_key {
        match fetch_arrowcloud_hard_ex_pane(chart_hash, arrowcloud_api_key, max_entries) {
            Ok(Some(pane)) => panes.insert(2.min(panes.len()), pane),
            Ok(None) => {}
            Err(error) => warn!(
                "ArrowCloud H.EX leaderboard fetch failed for chart {}: {}",
                chart_hash, error
            ),
        }
    }

    Ok(FetchedPlayerLeaderboards {
        data: PlayerLeaderboardData {
            panes,
            itl_self_score,
        },
        gs_entries,
    })
}

#[inline(always)]
const fn loading_player_leaderboard_snapshot() -> CachedPlayerLeaderboardData {
    CachedPlayerLeaderboardData {
        loading: true,
        data: None,
        error: None,
    }
}

#[inline(always)]
fn cache_snapshot_from_entry(entry: &PlayerLeaderboardCacheEntry) -> CachedPlayerLeaderboardData {
    match &entry.value {
        PlayerLeaderboardCacheValue::Ready(data) => CachedPlayerLeaderboardData {
            loading: false,
            data: Some(data.clone()),
            error: None,
        },
        PlayerLeaderboardCacheValue::Error(error) => CachedPlayerLeaderboardData {
            loading: false,
            data: None,
            error: Some(error.clone()),
        },
    }
}

#[inline(always)]
fn should_fetch_player_leaderboard_entry(
    entry: Option<&PlayerLeaderboardCacheEntry>,
    max_entries: usize,
    refresh_cached: bool,
) -> bool {
    let Some(entry) = entry else {
        return true;
    };
    let now = Instant::now();
    match entry.value {
        PlayerLeaderboardCacheValue::Ready(_) => {
            if refresh_cached {
                return entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after);
            }
            entry.max_entries < max_entries
                && entry
                    .retry_after
                    .is_none_or(|retry_after| now >= retry_after)
        }
        PlayerLeaderboardCacheValue::Error(_) => entry
            .retry_after
            .is_none_or(|retry_after| now >= retry_after),
    }
}

fn get_or_fetch_player_leaderboards_for_side_inner(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
    refresh_cached: bool,
) -> Option<CachedPlayerLeaderboardData> {
    let cfg = crate::config::get();
    if !cfg.enable_groovestats {
        return None;
    }
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() || max_entries == 0 {
        return None;
    }
    if !profile::is_session_side_joined(side) {
        return None;
    }

    let side_profile = profile::get_for_side(side);
    let gs_api_key = side_profile.groovestats_api_key.trim();
    if gs_api_key.is_empty() {
        return None;
    }
    let arrowcloud_api_key = side_profile.arrowcloud_api_key.trim().to_string();
    let include_arrowcloud = cfg.enable_arrowcloud
        && !arrowcloud_api_key.is_empty()
        && matches!(
            network::get_arrowcloud_status(),
            network::ArrowCloudConnectionStatus::Connected
        );
    let auto_populate = cfg.auto_populate_gs_scores;
    let auto_profile_id = if auto_populate {
        profile::active_local_profile_id_for_side(side)
    } else {
        None
    };
    let gs_username = side_profile.groovestats_username.trim().to_string();
    let should_auto_populate =
        auto_populate && auto_profile_id.is_some() && !gs_username.is_empty();

    let key = PlayerLeaderboardCacheKey {
        chart_hash: chart_hash.to_string(),
        api_key: gs_api_key.to_string(),
        arrowcloud_api_key,
        include_arrowcloud,
        show_ex_score: side_profile.show_ex_score,
    };
    let cached_snapshot = {
        let cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        cache.by_key.get(&key).map(cache_snapshot_from_entry)
    };

    match network::get_status() {
        network::ConnectionStatus::Pending => {
            return Some(cached_snapshot.unwrap_or_else(loading_player_leaderboard_snapshot));
        }
        network::ConnectionStatus::Connected(services) if !services.get_scores => {
            return Some(cached_snapshot.unwrap_or(CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: Some("Disabled".to_string()),
            }));
        }
        network::ConnectionStatus::Error(error) => {
            return Some(cached_snapshot.unwrap_or(CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: Some(error),
            }));
        }
        _ => {}
    }

    let mut should_spawn = false;
    let mut requested_max_entries = max_entries;
    let snapshot = {
        let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        let entry = cache.by_key.get(&key);
        if let Some(entry) = entry {
            requested_max_entries = requested_max_entries.max(entry.max_entries);
        }
        let snapshot = if let Some(entry) = entry {
            cache_snapshot_from_entry(entry)
        } else {
            loading_player_leaderboard_snapshot()
        };

        if !cache.in_flight.contains(&key)
            && should_fetch_player_leaderboard_entry(entry, requested_max_entries, refresh_cached)
        {
            cache.in_flight.insert(key.clone());
            should_spawn = true;
        }
        snapshot
    };

    if should_spawn {
        std::thread::spawn(move || {
            let fetched = fetch_player_leaderboards_internal(
                &key.chart_hash,
                &key.api_key,
                gs_username.as_str(),
                if key.include_arrowcloud {
                    Some(key.arrowcloud_api_key.as_str())
                } else {
                    None
                },
                key.show_ex_score,
                requested_max_entries,
            );
            let refresh_finished_at = Instant::now();
            let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
            cache.in_flight.remove(&key);

            match fetched {
                Ok(fetched) => {
                    set_cached_online_itl_self_score(
                        key.api_key.as_str(),
                        key.chart_hash.as_str(),
                        fetched.data.itl_self_score,
                    );
                    if should_auto_populate && let Some(profile_id) = auto_profile_id.as_deref() {
                        cache_gs_score_from_leaderboard(
                            profile_id,
                            gs_username.as_str(),
                            key.chart_hash.as_str(),
                            fetched.gs_entries.as_slice(),
                        );
                    }
                    cache.by_key.insert(
                        key,
                        PlayerLeaderboardCacheEntry {
                            value: PlayerLeaderboardCacheValue::Ready(fetched.data),
                            max_entries: requested_max_entries,
                            refreshed_at: refresh_finished_at,
                            retry_after: None,
                        },
                    );
                }
                Err(error) => {
                    if let Some(entry) = cache.by_key.get_mut(&key)
                        && matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_))
                    {
                        // Keep stale data visible on refresh failures, but back off retries.
                        entry.refreshed_at = refresh_finished_at;
                        entry.retry_after =
                            Some(refresh_finished_at + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL);
                    } else {
                        cache.by_key.insert(
                            key,
                            PlayerLeaderboardCacheEntry {
                                value: PlayerLeaderboardCacheValue::Error(error.to_string()),
                                max_entries: requested_max_entries,
                                refreshed_at: refresh_finished_at,
                                retry_after: Some(
                                    refresh_finished_at + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
                                ),
                            },
                        );
                    }
                }
            }
        });
    }

    Some(snapshot)
}

pub fn get_or_fetch_player_leaderboards_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    get_or_fetch_player_leaderboards_for_side_inner(chart_hash, side, max_entries, false)
}

pub fn get_cached_itl_self_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    let key = online_itl_self_score_key_for_side(chart_hash, side)?;
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .by_key
        .get(&key)
        .copied()
}

pub fn get_or_fetch_itl_self_score_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
) -> Option<u32> {
    if let Some(score) = get_cached_itl_self_score_for_side(chart_hash, side) {
        return Some(score);
    }
    const ITL_SELF_SCORE_FETCH_ENTRIES: usize = 3;
    let _ = get_or_fetch_player_leaderboards_for_side_inner(
        chart_hash,
        side,
        ITL_SELF_SCORE_FETCH_ENTRIES,
        false,
    )?;
    get_cached_itl_self_score_for_side(chart_hash, side)
}

pub fn refresh_player_leaderboards_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Option<CachedPlayerLeaderboardData> {
    get_or_fetch_player_leaderboards_for_side_inner(chart_hash, side, max_entries, true)
}
#[derive(Debug)]
struct MachineLeaderboardPlay {
    initials: String,
    score_percent: f64,
    played_at_ms: i64,
    is_fail: bool,
}

#[derive(Debug)]
struct MachineReplayPlay {
    initials: String,
    score_percent: f64,
    played_at_ms: i64,
    is_fail: bool,
    replay_beat0_time_seconds: f32,
    replay: Vec<LocalReplayEdgeV1>,
}

fn push_machine_leaderboard_from_dir(
    dir: &Path,
    chart_hash: &str,
    initials: &str,
    out: &mut Vec<MachineLeaderboardPlay>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(h) = read_local_score_header(&path) else {
            continue;
        };
        out.push(MachineLeaderboardPlay {
            initials: initials.to_string(),
            score_percent: h.score_percent,
            played_at_ms,
            is_fail: grade_from_code(h.grade_code) == Grade::Failed || h.fail_time.is_some(),
        });
    }
}

fn push_machine_replays_from_dir(
    dir: &Path,
    chart_hash: &str,
    initials: &str,
    out: &mut Vec<MachineReplayPlay>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_local_score_filename(name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(full) = read_local_score_entry(&path) else {
            continue;
        };
        out.push(MachineReplayPlay {
            initials: initials.to_string(),
            score_percent: full.score_percent,
            played_at_ms,
            is_fail: grade_from_code(full.grade_code) == Grade::Failed || full.fail_time.is_some(),
            replay_beat0_time_seconds: full.beat0_time_seconds,
            replay: full.replay,
        });
    }
}

fn local_score_date_string(played_at_ms: i64) -> String {
    let Some(dt) = Local.timestamp_millis_opt(played_at_ms).single() else {
        return String::new();
    };
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn get_machine_leaderboard_local(
    chart_hash: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays: Vec<MachineLeaderboardPlay> = Vec::new();
    for profile_meta in profile::scan_local_profiles() {
        let initials =
            profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
        let root = local_scores_root_for_profile(&profile_meta.id);
        push_machine_leaderboard_from_dir(&root, chart_hash, &initials, &mut plays);
        let shard_dir = root.join(shard2_for_hash(chart_hash));
        push_machine_leaderboard_from_dir(&shard_dir, chart_hash, &initials, &mut plays);
    }

    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.initials.cmp(&b.initials))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        out.push(LeaderboardEntry {
            rank: (i as u32).saturating_add(1),
            name: play.initials,
            machine_tag: None,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_rival: false,
            is_self: false,
            is_fail: play.is_fail,
        });
    }
    out
}

pub fn get_personal_leaderboard_local_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }
    let Some(profile_id) = profile::active_local_profile_id_for_side(side) else {
        return Vec::new();
    };

    let initials = profile_initials_for_id(&profile_id).unwrap_or_else(|| "----".to_string());
    let root = local_scores_root_for_profile(&profile_id);
    let shard_dir = root.join(shard2_for_hash(chart_hash));

    let mut plays: Vec<MachineLeaderboardPlay> = Vec::new();
    push_machine_leaderboard_from_dir(&root, chart_hash, &initials, &mut plays);
    push_machine_leaderboard_from_dir(&shard_dir, chart_hash, &initials, &mut plays);

    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.initials.cmp(&b.initials))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        out.push(LeaderboardEntry {
            rank: (i as u32).saturating_add(1),
            name: play.initials,
            machine_tag: None,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_rival: false,
            is_self: false,
            is_fail: play.is_fail,
        });
    }
    out
}

pub fn get_machine_replays_local(chart_hash: &str, max_entries: usize) -> Vec<MachineReplayEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays: Vec<MachineReplayPlay> = Vec::new();
    for profile_meta in profile::scan_local_profiles() {
        let initials =
            profile_initials_for_id(&profile_meta.id).unwrap_or_else(|| "----".to_string());
        let root = local_scores_root_for_profile(&profile_meta.id);
        push_machine_replays_from_dir(&root, chart_hash, &initials, &mut plays);
        let shard_dir = root.join(shard2_for_hash(chart_hash));
        push_machine_replays_from_dir(&shard_dir, chart_hash, &initials, &mut plays);
    }

    plays.sort_by(|a, b| {
        b.score_percent
            .partial_cmp(&a.score_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.played_at_ms.cmp(&a.played_at_ms))
            .then_with(|| a.initials.cmp(&b.initials))
    });

    let take_len = max_entries.min(plays.len());
    let mut out = Vec::with_capacity(take_len);
    for (i, play) in plays.into_iter().take(take_len).enumerate() {
        let mut replay = Vec::with_capacity(play.replay.len());
        for edge in play.replay {
            if !edge.event_music_time.is_finite() {
                continue;
            }
            let source = if edge.source == 1 {
                InputSource::Gamepad
            } else {
                InputSource::Keyboard
            };
            replay.push(ReplayEdge {
                event_music_time: edge.event_music_time,
                lane_index: edge.lane,
                pressed: edge.pressed,
                source,
            });
        }
        out.push(MachineReplayEntry {
            rank: (i as u32).saturating_add(1),
            name: play.initials,
            score: (play.score_percent * 10000.0).round(),
            date: local_score_date_string(play.played_at_ms),
            is_fail: play.is_fail,
            replay_beat0_time_seconds: play.replay_beat0_time_seconds,
            replay,
        });
    }
    out
}

// --- ITG PercentScore weights (mirror Simply Love SL_Init.lua, ITG mode) ---
const DP_W1: i32 = 5;
const DP_W2: i32 = 4;
const DP_W3: i32 = 2;
const DP_W4: i32 = 0;
const DP_W5: i32 = -6;
const DP_MISS: i32 = -12;
const DP_HELD: i32 = 5;

#[derive(Debug, Default, Clone, Copy)]
struct ParsedCommentCounts {
    w: u32,
    e: u32,
    g: u32,
    d: u32,
    wo: u32,
    m: u32,
}

fn parse_comment_counts(comment: &str) -> ParsedCommentCounts {
    let mut counts = ParsedCommentCounts::default();
    for part in comment.split(',') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }

        let mut value: u32 = 0;
        let mut idx = 0usize;
        for (i, ch) in s.char_indices() {
            if let Some(d) = ch.to_digit(10) {
                value = value.saturating_mul(10).saturating_add(d);
                idx = i + ch.len_utf8();
            } else {
                break;
            }
        }
        if value == 0 {
            continue;
        }

        let suffix = s[idx..].trim().to_ascii_lowercase();
        match suffix.as_str() {
            "w" => counts.w = value,
            "e" => counts.e = value,
            "g" => counts.g = value,
            "d" => counts.d = value,
            "wo" => counts.wo = value,
            "m" => counts.m = value,
            _ => {}
        }
    }
    counts
}

fn find_chart_stats_for_hash(chart_hash: &str) -> Option<rssp::stats::ArrowStats> {
    let cache = get_song_cache();
    for pack in cache.iter() {
        for song in &pack.songs {
            for chart in &song.charts {
                if chart.short_hash == chart_hash {
                    return Some(chart.stats.clone());
                }
            }
        }
    }
    None
}

fn compute_lamp_index(score: f64, comment: Option<&str>, chart_hash: &str) -> Option<u8> {
    let score_percent = score / 10000.0;

    // Perfect 100%: always at least a W1 full combo lamp.
    // Use a very small epsilon so only true 100.00% (score == 10000) hits this,
    // not 99.95% (score == 9995) or similar edge cases.
    if (score_percent - 1.0).abs() <= 1e-9 {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% -> Quad lamp (W1 FC, no DP check needed)",
            chart_hash,
            score_percent * 100.0
        );
        return Some(1);
    }

    let comment = if let Some(c) = comment {
        c
    } else {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% -> no lamp (no GrooveStats comment available)",
            chart_hash,
            score_percent * 100.0
        );
        return None;
    };
    let counts = parse_comment_counts(comment);

    // Any explicit Miss or Way Off disqualifies lamps immediately.
    if counts.m > 0 || counts.wo > 0 {
        return None;
    }

    let stats = if let Some(s) = find_chart_stats_for_hash(chart_hash) {
        s
    } else {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (chart stats not found for hash)",
            chart_hash,
            score_percent * 100.0,
            comment
        );
        return None;
    };
    let taps_rows = stats.total_steps as i32;
    let holds = stats.holds as i32;
    let rolls = stats.rolls as i32;

    if taps_rows <= 0 {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> no lamp (taps_rows <= 0, taps_rows={})",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows
        );
        return None;
    }

    // Reconstruct W1 count as "everything not explicitly listed".
    let non_w1_from_suffixes = counts.e + counts.g + counts.d + counts.wo + counts.m + counts.w;
    let inferred_w1 = if (non_w1_from_suffixes as i32) > taps_rows {
        0
    } else {
        (taps_rows as u32).saturating_sub(counts.e + counts.g + counts.d + counts.wo + counts.m)
    };
    let w1_total = counts.w.max(inferred_w1);

    // Dance Points from tap judgments (rows) only, per ITG PercentScoreWeight.
    let dp_taps: i32 = (w1_total as i32) * DP_W1
        + (counts.e as i32) * DP_W2
        + (counts.g as i32) * DP_W3
        + (counts.d as i32) * DP_W4
        + (counts.wo as i32) * DP_W5
        + (counts.m as i32) * DP_MISS;

    // Holds + rolls assumed fully held for the "no hidden errors" hypothesis.
    let dp_hold_roll: i32 = (holds + rolls) * DP_HELD;

    // Maximum possible DP if every tap was W1 and all holds/rolls fully held.
    let dp_possible_max: i32 = (taps_rows * DP_W1 + dp_hold_roll).max(1);
    let dp_expect_no_hidden_errors: i32 = dp_taps + dp_hold_roll;

    let dp_expect_frac = f64::from(dp_expect_no_hidden_errors) / f64::from(dp_possible_max);
    let dp_diff = (score_percent - dp_expect_frac).abs();
    let dp_consistent = dp_diff <= 0.0005;

    if !dp_consistent {
        // There must have been extra DP loss (e.g., dropped holds or hit mines).
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP mismatch: score%={:.5} vs no-hidden-errors%={:.5} (Δ={:.6}); \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> no lamp",
            chart_hash,
            score_percent * 100.0,
            comment,
            score_percent * 100.0,
            dp_expect_frac * 100.0,
            dp_diff * 100.0,
            taps_rows,
            holds,
            rolls,
            counts.w,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return None;
    }

    // At this point, we know there were no hidden hold/mine mistakes.
    // Classify the lamp tier, mirroring Simply Love's StageAward semantics.
    if counts.g == 0 && counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // Only W1/W2 present (and W1 reconstructed) => W2 full combo (FEC).
        if counts.e > 0 || w1_total > 0 {
            debug!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=FEC (index=2)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(2);
        }
    }

    if counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // At least one Great, but no Decents/WayOff/Miss => W3 full combo.
        if counts.g > 0 {
            debug!(
                "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W3 FC (index=3)",
                chart_hash,
                score_percent * 100.0,
                comment,
                taps_rows,
                holds,
                rolls,
                w1_total,
                counts.e,
                counts.g,
                counts.d,
                counts.wo,
                counts.m
            );
            return Some(3);
        }
    }

    // No WayOff/Miss and DP-consistent => at worst a W4 full combo.
    if counts.wo == 0 && counts.m == 0 {
        debug!(
            "GrooveStats lamp: hash={} score={:.4}% comment=\"{}\" -> DP ok (no hidden errors). \
taps_rows={} holds={} rolls={} counts[w={}, e={}, g={}, d={}, wo={}, m={}] -> lamp=W4 FC (index=4)",
            chart_hash,
            score_percent * 100.0,
            comment,
            taps_rows,
            holds,
            rolls,
            w1_total,
            counts.e,
            counts.g,
            counts.d,
            counts.wo,
            counts.m
        );
        return Some(4);
    }

    None
}

fn compute_lamp_judge_count(lamp_index: Option<u8>, comment: Option<&str>) -> Option<u8> {
    let lamp_index = lamp_index?;
    let comment = comment?;
    let counts = parse_comment_counts(comment);

    // zmod-style single-digit overlay:
    // - lamp 2 shows #W2
    // - lamp 3 shows #W3
    // (lamp 1 would show FA+ blue W1 count, which we don't track yet)
    let count = match lamp_index {
        2 => counts.e,
        3 => counts.g,
        _ => return None,
    };
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

// --- Grade Calculation ---

pub fn score_to_grade(score: f64) -> Grade {
    let percent = score / 10000.0;
    if percent >= 1.00 {
        Grade::Tier01
    }
    // Note: We don't have enough info to detect Quints (W0) yet.
    else if percent >= 0.99 {
        Grade::Tier02
    }
    // three-stars
    else if percent >= 0.98 {
        Grade::Tier03
    }
    // two-stars
    else if percent >= 0.96 {
        Grade::Tier04
    }
    // one-star
    else if percent >= 0.94 {
        Grade::Tier05
    }
    // s-plus
    else if percent >= 0.92 {
        Grade::Tier06
    }
    // s
    else if percent >= 0.89 {
        Grade::Tier07
    }
    // s-minus
    else if percent >= 0.86 {
        Grade::Tier08
    }
    // a-plus
    else if percent >= 0.83 {
        Grade::Tier09
    }
    // a
    else if percent >= 0.80 {
        Grade::Tier10
    }
    // a-minus
    else if percent >= 0.76 {
        Grade::Tier11
    }
    // b-plus
    else if percent >= 0.72 {
        Grade::Tier12
    }
    // b
    else if percent >= 0.68 {
        Grade::Tier13
    }
    // b-minus
    else if percent >= 0.64 {
        Grade::Tier14
    }
    // c-plus
    else if percent >= 0.60 {
        Grade::Tier15
    }
    // c
    else if percent >= 0.55 {
        Grade::Tier16
    }
    // c-minus
    else {
        Grade::Tier17
    } // d
    // Grade::Failed is not score-based; it's determined by gameplay failure (e.g., lifebar empty),
    // which is not yet implemented. This function will never return Grade::Failed.
}

// --- Public Fetch Function ---

const SCORE_IMPORT_RATE_LIMIT_PER_SECOND: u32 = 3;
const SCORE_IMPORT_REQUEST_INTERVAL: Duration = Duration::from_millis(334);
const SCORE_IMPORT_PROGRESS_LOG_EVERY: usize = 100;
const SCORE_IMPORT_GS_BASE_URL: &str = "https://api.groovestats.com";
const SCORE_IMPORT_BS_BASE_URL: &str = "https://boogiestats.andr.host";
const SCORE_IMPORT_AC_BASE_URL: &str = "https://api.arrowcloud.dance";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreImportEndpoint {
    GrooveStats,
    BoogieStats,
    ArrowCloud,
}

impl ScoreImportEndpoint {
    #[inline(always)]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::GrooveStats => "GrooveStats",
            Self::BoogieStats => "BoogieStats",
            Self::ArrowCloud => "ArrowCloud",
        }
    }

    #[inline(always)]
    pub const fn requires_username(self) -> bool {
        !matches!(self, Self::ArrowCloud)
    }

    fn player_leaderboards_url(self) -> String {
        let base = match self {
            Self::GrooveStats => SCORE_IMPORT_GS_BASE_URL,
            Self::BoogieStats => SCORE_IMPORT_BS_BASE_URL,
            Self::ArrowCloud => SCORE_IMPORT_AC_BASE_URL,
        };
        format!("{}/player-leaderboards.php", base.trim_end_matches('/'))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ScoreBulkImportSummary {
    pub requested_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub rate_limit_per_second: u32,
    pub elapsed_seconds: f32,
    pub canceled: bool,
}

#[derive(Debug, Clone)]
pub struct ScoreImportProgress {
    pub processed_charts: usize,
    pub total_charts: usize,
    pub imported_scores: usize,
    pub missing_scores: usize,
    pub failed_requests: usize,
    pub detail: String,
}

fn score_import_api_key_for_endpoint<'a>(
    endpoint: ScoreImportEndpoint,
    profile: &'a Profile,
) -> &'a str {
    match endpoint {
        ScoreImportEndpoint::GrooveStats | ScoreImportEndpoint::BoogieStats => {
            profile.groovestats_api_key.trim()
        }
        ScoreImportEndpoint::ArrowCloud => profile.arrowcloud_api_key.trim(),
    }
}

fn score_entry_matches_profile(
    entry: &LeaderboardApiEntry,
    endpoint: ScoreImportEndpoint,
    username: &str,
) -> bool {
    if entry.is_self {
        return true;
    }
    if endpoint.requires_username() {
        return entry.name.eq_ignore_ascii_case(username);
    }
    false
}

fn fetch_player_score_from_endpoint(
    endpoint: ScoreImportEndpoint,
    profile: &Profile,
    chart_hash: &str,
) -> Result<Option<CachedScore>, Box<dyn Error + Send + Sync>> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return Err("Missing chart hash for score request.".into());
    }

    let api_key = score_import_api_key_for_endpoint(endpoint, profile);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    let username = profile.groovestats_username.trim();
    if endpoint.requires_username() && username.is_empty() {
        return Err(format!(
            "{} username is missing in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let agent = network::get_agent();
    let api_url = endpoint.player_leaderboards_url();
    let response = agent
        .get(&api_url)
        .header("x-api-key-player-1", api_key)
        .query("chartHashP1", chart_hash)
        .call()?;

    if response.status() != 200 {
        return Err(format!("API returned status {}", response.status()).into());
    }

    let decoded: LeaderboardsApiResponse = response.into_body().read_json()?;
    let score_opt = decoded
        .player1
        .map_or_else(Vec::new, |p1| p1.gs_leaderboard)
        .into_iter()
        .find(|entry| score_entry_matches_profile(entry, endpoint, username))
        .map(|entry| {
            cached_score_from_gs(
                entry.score,
                entry.comments.as_deref(),
                chart_hash,
                entry.is_fail,
            )
        });

    Ok(score_opt)
}

fn fetch_player_score_from_api(
    profile: &Profile,
    chart_hash: &str,
) -> Result<Option<CachedScore>, Box<dyn Error + Send + Sync>> {
    let endpoint = if crate::core::network::is_boogiestats_active() {
        ScoreImportEndpoint::BoogieStats
    } else {
        ScoreImportEndpoint::GrooveStats
    };
    fetch_player_score_from_endpoint(endpoint, profile, chart_hash)
}

fn collect_chart_hashes_for_import(
    pack_group_filter: Option<&str>,
    profile_id: &str,
    only_missing_gs_scores: bool,
) -> Vec<String> {
    let filter_norm = pack_group_filter
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_ascii_lowercase);
    let existing_scores = if only_missing_gs_scores {
        cached_gs_chart_hashes_for_profile(profile_id)
    } else {
        HashSet::new()
    };

    let mut chart_hashes = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let song_cache = get_song_cache();
    for pack in song_cache.iter() {
        let group_name = pack.group_name.trim();
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };
        if let Some(filter) = filter_norm.as_deref()
            && group_name.to_ascii_lowercase() != filter
            && display_name.to_ascii_lowercase() != filter
        {
            continue;
        }

        for song in &pack.songs {
            for chart in &song.charts {
                let chart_hash = chart.short_hash.trim();
                if chart_hash.is_empty() {
                    continue;
                }
                if only_missing_gs_scores && existing_scores.contains(chart_hash) {
                    continue;
                }
                if seen.insert(chart_hash.to_string()) {
                    chart_hashes.push(chart_hash.to_string());
                }
            }
        }
    }
    chart_hashes
}

#[inline(always)]
fn wait_for_next_import_request(last_request_started_at: Option<Instant>) {
    let Some(last_started) = last_request_started_at else {
        return;
    };
    let elapsed = last_started.elapsed();
    if elapsed < SCORE_IMPORT_REQUEST_INTERVAL {
        std::thread::sleep(SCORE_IMPORT_REQUEST_INTERVAL - elapsed);
    }
}

pub fn import_scores_for_profile<F>(
    endpoint: ScoreImportEndpoint,
    profile_id: String,
    profile: Profile,
    pack_group: Option<String>,
    only_missing_gs_scores: bool,
    on_progress: F,
    should_cancel: impl Fn() -> bool,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(ScoreImportProgress),
{
    let mut on_progress = on_progress;
    let api_key = score_import_api_key_for_endpoint(endpoint, &profile);
    if api_key.is_empty() {
        return Err(format!(
            "{} API key is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }
    if endpoint.requires_username() && profile.groovestats_username.trim().is_empty() {
        return Err(format!(
            "{} username is not set in profile configuration.",
            endpoint.display_name()
        )
        .into());
    }

    let username = profile.groovestats_username.trim().to_string();
    let chart_hashes =
        collect_chart_hashes_for_import(pack_group.as_deref(), &profile_id, only_missing_gs_scores);
    let requested_charts = chart_hashes.len();
    let filter_note = if only_missing_gs_scores {
        " (missing GS only)"
    } else {
        ""
    };
    on_progress(ScoreImportProgress {
        processed_charts: 0,
        total_charts: requested_charts,
        imported_scores: 0,
        missing_scores: 0,
        failed_requests: 0,
        detail: format!(
            "Queued {requested_charts} chart hashes for {} import{}.",
            endpoint.display_name(),
            filter_note
        ),
    });
    if requested_charts == 0 {
        return Ok(ScoreBulkImportSummary {
            requested_charts: 0,
            imported_scores: 0,
            missing_scores: 0,
            failed_requests: 0,
            rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
            elapsed_seconds: 0.0,
            canceled: false,
        });
    }

    let import_started = Instant::now();
    let mut last_request_started_at: Option<Instant> = None;
    let mut imported_scores = 0usize;
    let mut missing_scores = 0usize;
    let mut failed_requests = 0usize;
    let mut canceled = false;

    for (idx, chart_hash) in chart_hashes.iter().enumerate() {
        if should_cancel() {
            canceled = true;
            debug!(
                "{} score import canceled after {idx}/{requested_charts} charts.",
                endpoint.display_name()
            );
            break;
        }
        wait_for_next_import_request(last_request_started_at);
        if should_cancel() {
            canceled = true;
            debug!(
                "{} score import canceled after {idx}/{requested_charts} charts.",
                endpoint.display_name()
            );
            break;
        }
        last_request_started_at = Some(Instant::now());
        let detail = match fetch_player_score_from_endpoint(endpoint, &profile, chart_hash) {
            Ok(Some(score)) => {
                cache_gs_score_for_profile(&profile_id, chart_hash, score, username.as_str());
                imported_scores += 1;
                format!(
                    "Found {} score for {} on {}.",
                    endpoint.display_name(),
                    username,
                    chart_hash
                )
            }
            Ok(None) => {
                missing_scores += 1;
                format!(
                    "No {} score for {} on {}.",
                    endpoint.display_name(),
                    username,
                    chart_hash
                )
            }
            Err(e) => {
                failed_requests += 1;
                let msg = format!(
                    "{} import request failed for chart {}: {}",
                    endpoint.display_name(),
                    chart_hash,
                    e
                );
                warn!("{msg}");
                msg
            }
        };

        let done = idx + 1;
        on_progress(ScoreImportProgress {
            processed_charts: done,
            total_charts: requested_charts,
            imported_scores,
            missing_scores,
            failed_requests,
            detail: detail.clone(),
        });
        if done == requested_charts || done % SCORE_IMPORT_PROGRESS_LOG_EVERY == 0 || done == 1 {
            debug!(
                "{} bulk import progress for '{}': {done}/{requested_charts} charts (imported={}, missing={}, failed={})",
                endpoint.display_name(),
                username,
                imported_scores,
                missing_scores,
                failed_requests
            );
        }
        debug!("{detail}");
    }

    Ok(ScoreBulkImportSummary {
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
        elapsed_seconds: import_started.elapsed().as_secs_f32(),
        canceled,
    })
}

pub fn fetch_and_store_grade(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if profile.groovestats_api_key.trim().is_empty()
        || profile.groovestats_username.trim().is_empty()
    {
        return Err("GrooveStats API key or username is not set in profile.ini.".into());
    }

    debug!(
        "Requesting scores for '{}' on chart '{}'...",
        profile.groovestats_username, chart_hash
    );

    if let Some(cached_score) = fetch_player_score_from_api(&profile, chart_hash.as_str())? {
        cache_gs_score_for_profile(
            &profile_id,
            &chart_hash,
            cached_score,
            profile.groovestats_username.trim(),
        );
    } else {
        warn!(
            "No score found for player '{}' on chart '{}'. Caching as Failed.",
            profile.groovestats_username, chart_hash
        );
        set_cached_gs_score_for_profile(&profile_id, chart_hash, cached_failed_gs_score());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::chart::{ChartData, StaminaCounts};
    use crate::game::scroll::ScrollSpeedSetting;
    use crate::game::song::SongData;
    use crate::game::timing::ScatterPoint;
    use rssp::{TechCounts, stats::ArrowStats};
    use serde_json::{Value, json};
    use std::path::PathBuf;

    fn sample_scatter(time_sec: f32, offset_ms: Option<f32>) -> ScatterPoint {
        ScatterPoint {
            time_sec,
            offset_ms,
            direction_code: 1,
            is_stream: false,
            is_left_foot: false,
            miss_because_held: false,
        }
    }

    fn sample_chart(chart_type: &str) -> ChartData {
        ChartData {
            chart_type: chart_type.to_string(),
            difficulty: "Challenge".to_string(),
            description: String::new(),
            chart_name: String::new(),
            meter: 12,
            step_artist: String::new(),
            short_hash: "deadbeefcafebabe".to_string(),
            stats: ArrowStats::default(),
            tech_counts: TechCounts::default(),
            mines_nonfake: 12,
            stamina_counts: StaminaCounts::default(),
            total_streams: 0,
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
            has_significant_timing_changes: true,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 12,
        }
    }

    fn sample_song(dir: &str) -> SongData {
        SongData {
            simfile_path: PathBuf::from(dir).join("song.ssc"),
            title: "Song".to_string(),
            subtitle: String::new(),
            translit_title: String::new(),
            translit_subtitle: String::new(),
            artist: String::new(),
            banner_path: None,
            background_path: None,
            background_changes: Vec::new(),
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
    fn arrowcloud_timing_data_keeps_miss_rows() {
        let scatter = [
            sample_scatter(12.5, Some(8.0)),
            sample_scatter(12.75, None),
            sample_scatter(f32::NAN, Some(2.0)),
        ];
        let timing_data = arrowcloud_timing_data_from_scatter(&scatter);
        assert_eq!(timing_data.len(), 2);

        let value = serde_json::to_value(&timing_data).expect("serialize timingData");
        assert_eq!(value[0][0], json!(12.5));
        let first_offset = value[0][1]
            .as_f64()
            .expect("timingData[0][1] should be numeric");
        assert!((first_offset - 0.008).abs() < 1e-6);
        assert_eq!(value[1][0], json!(12.75));
        assert_eq!(value[1][1], json!("Miss"));
    }

    #[test]
    fn arrowcloud_payload_serializes_miss_and_counts() {
        let payload = ArrowCloudPayload {
            song_name: "Test Song".to_string(),
            artist: "Test Artist".to_string(),
            pack: "Test Pack".to_string(),
            length: "1:23".to_string(),
            hash: "deadbeefcafebabe".to_string(),
            timing_data: vec![(24.488_208_770_752, ArrowCloudTimingOffset::Miss("Miss"))],
            difficulty: 12,
            stepartist: "Tester".to_string(),
            radar: ArrowCloudRadar {
                holds: [1, 2],
                mines: [3, 4],
                rolls: [5, 6],
            },
            judgment_counts: ArrowCloudJudgmentCounts {
                fantastic_plus: 10,
                fantastic: 20,
                excellent: 30,
                great: 40,
                decent: 50,
                way_off: 60,
                miss: 3,
                total_steps: 213,
                holds_held: 1,
                total_holds: 2,
                mines_hit: 3,
                total_mines: 4,
                rolls_held: 5,
                total_rolls: 6,
            },
            nps_info: ArrowCloudNpsInfo {
                peak_nps: 0.0,
                points: Vec::new(),
            },
            lifebar_info: Vec::new(),
            modifiers: ArrowCloudModifiers {
                visual_delay: 0,
                acceleration: Vec::new(),
                appearance: Vec::new(),
                effect: Vec::new(),
                mini: 0,
                turn: "None".to_string(),
                disabled_windows: "None".to_string(),
                speed: ArrowCloudSpeed {
                    value: 600.0,
                    speed_type: "C",
                },
                perspective: "Overhead".to_string(),
                noteskin: "cel".to_string(),
                scroll: None,
            },
            music_rate: 1.0,
            used_autoplay: false,
            passed: true,
            body_version: ARROWCLOUD_BODY_VERSION,
            arrow_cloud_body_version: ARROWCLOUD_BODY_VERSION,
            engine_name: ARROWCLOUD_ENGINE_NAME,
            engine_version: ARROWCLOUD_ENGINE_VERSION,
        };

        let value = serde_json::to_value(&payload).expect("serialize ArrowCloud payload");
        assert_eq!(value["timingData"][0][1], json!("Miss"));
        assert_eq!(value["judgmentCounts"]["miss"], json!(3));
        assert_eq!(value["judgmentCounts"]["wayOff"], json!(60));
        assert_eq!(value["bodyVersion"], Value::String("1.4".to_string()));
        assert_eq!(
            value["_arrowCloudBodyVersion"],
            Value::String("1.4".to_string())
        );
    }

    #[test]
    fn groovestats_payload_serializes_old_api_shape() {
        let payload = GrooveStatsSubmitPlayerPayload {
            rate: 150,
            score: 9_975,
            judgment_counts: GrooveStatsJudgmentCounts {
                fantastic_plus: 7,
                fantastic: 12,
                excellent: 18,
                great: 4,
                decent: 1,
                way_off: 0,
                miss: 2,
                total_steps: 213,
                holds_held: 5,
                total_holds: 6,
                mines_hit: 1,
                total_mines: 8,
                rolls_held: 2,
                total_rolls: 3,
            },
            rescore_counts: GrooveStatsRescoreCounts {
                fantastic_plus: 1,
                fantastic: 2,
                excellent: 3,
                great: 4,
                decent: 5,
                way_off: 6,
            },
            used_cmod: true,
            comment: "[DS], FA+, 99.50EX, 2w, 1m, C650".to_string(),
        };

        let value = serde_json::to_value(&payload).expect("serialize GrooveStats submit payload");
        assert_eq!(value["rate"], json!(150));
        assert_eq!(value["score"], json!(9_975));
        assert_eq!(value["judgmentCounts"]["fantasticPlus"], json!(7));
        assert_eq!(value["judgmentCounts"]["totalMines"], json!(8));
        assert_eq!(value["rescoreCounts"]["wayOff"], json!(6));
        assert_eq!(value["usedCmod"], json!(true));
        assert_eq!(value["comment"], json!("[DS], FA+, 99.50EX, 2w, 1m, C650"));
    }

    #[test]
    fn groovestats_manual_qr_url_preserves_base_url_case() {
        let counts = GrooveStatsJudgmentCounts {
            fantastic_plus: 0x0a,
            fantastic: 0x0b,
            excellent: 0x0c,
            great: 0x0d,
            decent: 0x0e,
            way_off: 0x0f,
            miss: 0x10,
            total_steps: 0x1d,
            holds_held: 0x11,
            total_holds: 0x12,
            mines_hit: 0x15,
            total_mines: 0x16,
            rolls_held: 0x13,
            total_rolls: 0x14,
        };
        let rescored = GrooveStatsRescoreCounts {
            fantastic_plus: 0x01,
            fantastic: 0x02,
            excellent: 0x03,
            great: 0x04,
            decent: 0x05,
            way_off: 0x06,
        };

        let url = groovestats_manual_qr_url(
            "https://www.groovestats.com",
            "deadbeef",
            3,
            &counts,
            &rescored,
            true,
            150,
            true,
        )
        .expect("manual qr url");

        assert_eq!(
            url,
            "https://www.groovestats.com/QR/deadbeef/T1dGaHbIcJdKeLfM10H11T12R13T14M15T16G1H2I3J4K5L6/F1R96C1V3"
        );
    }

    #[test]
    fn groovestats_comment_counts_ignore_ds_prefix() {
        let counts = parse_comment_counts("[DS], 15e, 2g, 2m");
        assert_eq!(counts.e, 15);
        assert_eq!(counts.g, 2);
        assert_eq!(counts.m, 2);
    }

    #[test]
    fn groovestats_lamp_judge_count_ignores_ds_prefix() {
        assert_eq!(compute_lamp_judge_count(Some(2), Some("[DS], 5e")), Some(5));
        assert_eq!(compute_lamp_judge_count(Some(3), Some("[DS], 2g")), Some(2));
    }

    #[test]
    fn groovestats_validity_allows_cmod_and_no_mines() {
        let mut profile = Profile::default();
        profile.scroll_speed = ScrollSpeedSetting::CMod(650.0);
        profile.remove_active_mask = 1u8 << 1;

        assert_eq!(
            groovestats_submit_invalid_reason(&sample_chart("dance-single"), false, &profile, 1.5),
            None
        );
    }

    #[test]
    fn groovestats_validity_rejects_custom_window_and_solo() {
        let mut profile = Profile::default();
        profile.custom_fantastic_window = true;

        assert_eq!(
            groovestats_submit_invalid_reason(&sample_chart("dance-single"), false, &profile, 1.0),
            Some("Metrics or preferences are incorrect.".to_string())
        );
        assert_eq!(
            groovestats_submit_invalid_reason(
                &sample_chart("dance-solo"),
                false,
                &Profile::default(),
                1.0
            ),
            Some("GrooveStats does not support dance-solo charts.".to_string())
        );
    }

    #[test]
    fn groovestats_validity_rejects_lua_simfiles() {
        assert_eq!(
            groovestats_submit_invalid_reason(
                &sample_chart("dance-single"),
                true,
                &Profile::default(),
                1.0,
            ),
            Some("simfile relies on lua".to_string())
        );
    }

    #[test]
    fn parse_itl_points_reads_chart_name_values() {
        assert_eq!(
            parse_itl_points("7500 (P) + 12000 (S)"),
            Some((7500, 12000))
        );
        assert_eq!(parse_itl_points("No points here"), None);
    }

    #[test]
    fn itl_points_for_chart_uses_chart_name_curve() {
        let mut chart = sample_chart("dance-single");
        chart.chart_name = "7500 (P) + 12000 (S)".to_string();

        assert_eq!(itl_points_for_chart(&chart, 10_000), Some(19_500));
    }

    #[test]
    fn itl_points_curve_keeps_full_ex_exact() {
        assert_eq!(itl_points_for_song(7500, 12000, 100.0), 19_500);
    }

    #[test]
    fn itl_judgments_compare_from_top_window() {
        let prev = ItlJudgments {
            w0: 10,
            w1: 20,
            ..ItlJudgments::default()
        };
        let better = ItlJudgments {
            w0: 11,
            w1: 19,
            ..ItlJudgments::default()
        };
        let worse = ItlJudgments {
            w0: 9,
            w1: 25,
            ..ItlJudgments::default()
        };

        assert!(itl_judgments_better(&better, &prev));
        assert!(!itl_judgments_better(&worse, &prev));
    }

    #[test]
    fn itl_file_reads_simply_love_and_legacy_ex_values() {
        let sl: ItlFileData = serde_json::from_value(json!({
            "hashMap": {
                "sl": { "ex": 9437 }
            }
        }))
        .unwrap();
        let legacy: ItlFileData = serde_json::from_value(json!({
            "hashMap": {
                "legacy": { "ex": 94.37 }
            }
        }))
        .unwrap();

        assert_eq!(sl.hash_map["sl"].ex, 9437);
        assert_eq!(legacy.hash_map["legacy"].ex, 9437);
    }

    #[test]
    fn itl_score_lookup_uses_song_path_map() {
        let song = sample_song("/Songs/ITL Online 2026/Example");
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/ITL Online 2026/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );
        data.hash_map.insert(
            "deadbeefcafebabe".to_string(),
            ItlHashEntry {
                ex: 9754,
                clear_type: 4,
                points: 12345,
                ..ItlHashEntry::default()
            },
        );

        assert_eq!(
            itl_score_for_song(&song, &data),
            Some(CachedItlScore {
                ex_hundredths: 9754,
                clear_type: 4,
                points: 12345,
            })
        );
    }

    #[test]
    fn itl_chart_no_cmod_uses_subtitle_fallback() {
        let mut song = sample_song("/Songs/ITL Online 2026/Example");
        song.subtitle = "(NO CMOD)".to_string();

        assert!(itl_chart_no_cmod(&song, None));
    }

    #[test]
    fn itl_is_song_accepts_cached_path_map_without_group_lookup() {
        let song = sample_song("/Songs/Custom Pack/Example");
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/Custom Pack/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );

        assert!(itl_is_song(
            &song,
            Some("/Songs/Custom Pack/Example"),
            &data
        ));
    }

    #[test]
    fn itl_totals_split_song_and_ex_points() {
        let mut data = ItlFileData::default();
        data.hash_map.insert(
            "a".to_string(),
            ItlHashEntry {
                points: 100,
                passing_points: 60,
                steps_type: "single".to_string(),
                ..ItlHashEntry::default()
            },
        );
        data.hash_map.insert(
            "b".to_string(),
            ItlHashEntry {
                points: 50,
                passing_points: 20,
                steps_type: "double".to_string(),
                ..ItlHashEntry::default()
            },
        );

        itl_rebuild_song_ranks(&mut data);

        assert_eq!(data.points, vec![100, 50]);
        assert_eq!(data.hash_map["a"].rank, Some(1));
        assert_eq!(data.hash_map["b"].rank, Some(1));
        assert_eq!(
            itl_point_totals(&data),
            ItlPointTotals {
                ranking_points: 150,
                song_points: 80,
                ex_points: 70,
                total_points: 150,
            }
        );
    }

    #[test]
    fn leaderboard_self_score_prefers_self_flag_for_itl() {
        let entries = vec![
            LeaderboardApiEntry {
                rank: 10,
                name: "Other".to_string(),
                machine_tag: None,
                score: 9321.0,
                date: String::new(),
                is_rival: false,
                is_self: false,
                is_fail: false,
                comments: None,
            },
            LeaderboardApiEntry {
                rank: 25,
                name: "Player".to_string(),
                machine_tag: None,
                score: 9789.0,
                date: String::new(),
                is_rival: false,
                is_self: true,
                is_fail: false,
                comments: None,
            },
        ];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "ignored"),
            Some(9789)
        );
    }

    #[test]
    fn leaderboard_self_score_falls_back_to_username_match() {
        let entries = vec![LeaderboardApiEntry {
            rank: 25,
            name: "PerfectTaste".to_string(),
            machine_tag: None,
            score: 9712.0,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
            comments: None,
        }];

        assert_eq!(
            leaderboard_self_score_10000(&entries, "perfecttaste"),
            Some(9712)
        );
    }

    #[test]
    fn player_leaderboard_cache_reuses_success_until_more_rows_are_needed() {
        let ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
            }),
            max_entries: 5,
            refreshed_at: Instant::now(),
            retry_after: None,
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            5,
            false
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&ready),
            3,
            false
        ));
        assert!(should_fetch_player_leaderboard_entry(
            Some(&ready),
            10,
            false
        ));
        assert!(should_fetch_player_leaderboard_entry(Some(&ready), 5, true));

        let cooled_down_ready = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Ready(PlayerLeaderboardData {
                panes: Vec::new(),
                itl_self_score: None,
            }),
            max_entries: 5,
            refreshed_at: Instant::now(),
            retry_after: Some(Instant::now() + PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL),
        };
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            10,
            false
        ));
        assert!(!should_fetch_player_leaderboard_entry(
            Some(&cooled_down_ready),
            5,
            true
        ));

        let stale_error = PlayerLeaderboardCacheEntry {
            value: PlayerLeaderboardCacheValue::Error("boom".to_string()),
            max_entries: 5,
            refreshed_at: Instant::now() - PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
            retry_after: Some(Instant::now() - Duration::from_millis(1)),
        };
        assert!(should_fetch_player_leaderboard_entry(
            Some(&stale_error),
            5,
            false
        ));
    }
}
