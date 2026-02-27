use crate::config::SimpleIni;
use crate::core::input::InputSource;
use crate::core::network;
use crate::game::gameplay;
use crate::game::judgment;
use crate::game::profile::{self, Profile};
use crate::game::song::get_song_cache;
use crate::game::stage_stats;
use chrono::{Local, TimeZone};
use log::{info, warn};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
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

// --- GrooveStats grade cache (on-disk + network-fetched) ---

#[derive(Default)]
struct GsScoreCacheState {
    loaded_profiles: HashMap<String, HashMap<String, CachedScore>>,
}

static GS_SCORE_CACHE: std::sync::LazyLock<Mutex<GsScoreCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(GsScoreCacheState::default()));

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
        info!(
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
    info!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
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
        info!(
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
            info!("Loaded machine local score cache: {total} chart(s) in {load_ms:.2}ms.");
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
    info!("Prewarmed SelectMusic score caches in {elapsed_ms:.2}ms.");
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
                info!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
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
        info!("Skipping local score save: autoplay was used during this stage.");
        return;
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Parameter retained for parity with Simply Love helpers; currently unused.
    let mines_disabled = false;

    for player_idx in 0..gs.num_players {
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

const ARROWCLOUD_BODY_VERSION: &str = "1.3";
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
struct ArrowCloudPayload {
    #[serde(rename = "songName")]
    song_name: String,
    artist: String,
    pack: String,
    length: String,
    hash: String,
    #[serde(rename = "timingData")]
    timing_data: Vec<[f64; 2]>,
    difficulty: u32,
    stepartist: String,
    radar: ArrowCloudRadar,
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
fn arrowcloud_timing_data(gs: &gameplay::State, player_idx: usize) -> Vec<[f64; 2]> {
    let (start, end) = gs.note_ranges[player_idx];
    let notes = &gs.notes[start..end];
    let note_times = &gs.note_time_cache[start..end];
    let col_offset = player_idx.saturating_mul(gs.cols_per_player);
    let scatter = crate::game::timing::build_scatter_points(
        notes,
        note_times,
        col_offset,
        gs.cols_per_player,
        &gs.mini_indicator_stream_segments[player_idx],
    );

    let mut out = Vec::with_capacity(scatter.len());
    for point in scatter {
        let Some(offset_ms) = point.offset_ms else {
            continue;
        };
        if !point.time_sec.is_finite() || !offset_ms.is_finite() {
            continue;
        }
        out.push([point.time_sec as f64, (offset_ms / 1000.0) as f64]);
    }
    out
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
        let t = chart.timing.get_time_for_beat((measure as f32) * 4.0);
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
fn arrowcloud_safe_hash(hash: &str) -> String {
    let mut out = String::with_capacity(hash.len());
    for ch in hash.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        }
    }
    if out.is_empty() {
        out.push_str("unknownhash");
    }
    out
}

fn write_arrowcloud_payload_dump(
    side: profile::PlayerSide,
    chart_hash: &str,
    payload: &ArrowCloudPayload,
) -> Result<PathBuf, String> {
    let side_tag = match side {
        profile::PlayerSide::P1 => "p1",
        profile::PlayerSide::P2 => "p2",
    };
    let timestamp = Local::now().format("%Y%m%d-%H%M%S-%3f");
    let safe_hash = arrowcloud_safe_hash(chart_hash);
    let base_dir = PathBuf::from("save").join("arrowcloud");
    let payload_dir = base_dir.join("payloads");
    let payload_path = payload_dir.join(format!("{timestamp}-{side_tag}-{safe_hash}.json"));
    let latest_path = base_dir.join(format!("latest-{side_tag}.json"));

    fs::create_dir_all(&payload_dir).map_err(|e| {
        format!(
            "failed to create ArrowCloud payload dir '{}': {e}",
            payload_dir.display()
        )
    })?;
    let bytes = serde_json::to_vec_pretty(payload)
        .map_err(|e| format!("failed to serialize ArrowCloud payload JSON: {e}"))?;
    fs::write(&payload_path, &bytes).map_err(|e| {
        format!(
            "failed to write ArrowCloud payload dump '{}': {e}",
            payload_path.display()
        )
    })?;
    if let Err(e) = fs::write(&latest_path, &bytes) {
        warn!(
            "Failed to update ArrowCloud latest payload '{}' : {e}",
            latest_path.display()
        );
    }

    Ok(payload_path)
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
fn arrowcloud_log_snippet(text: &str) -> String {
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
        let snippet = arrowcloud_log_snippet(body.as_str());
        if !snippet.is_empty() {
            info!(
                "ArrowCloud submit success for {:?} ({}) status={} body='{}'",
                side,
                payload.hash,
                status_code,
                snippet.as_str()
            );
        } else {
            info!(
                "ArrowCloud submit success for {:?} ({}) status={}",
                side, payload.hash, status_code
            );
        }
        return Ok(());
    }

    let snippet = arrowcloud_log_snippet(body.as_str());
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

pub fn dump_arrowcloud_payloads_from_gameplay(gs: &gameplay::State) {
    if !crate::config::get().enable_arrowcloud || gs.num_players == 0 {
        return;
    }

    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx) else {
            continue;
        };
        let side = gameplay_side_for_player(gs, player_idx);
        match write_arrowcloud_payload_dump(side, payload.hash.as_str(), &payload) {
            Ok(path) => info!(
                "Saved ArrowCloud payload dump for {:?} ({}) to '{}'",
                side,
                payload.hash,
                path.display()
            ),
            Err(e) => warn!(
                "Failed to save ArrowCloud payload dump for {:?} ({}) : {}",
                side, payload.hash, e
            ),
        }
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
        info!("Skipping ArrowCloud submit: autoplay/replay was used.");
        return;
    }
    if gs.course_display_totals.is_some() && !cfg.autosubmit_course_scores_individually {
        info!("Skipping ArrowCloud submit: course per-song autosubmit is disabled.");
        return;
    }
    if let network::ArrowCloudConnectionStatus::Error(msg) = network::get_arrowcloud_status() {
        warn!("Skipping ArrowCloud submit due to connection status error: {msg}");
        return;
    }

    let mut jobs = Vec::with_capacity(gs.num_players.min(gameplay::MAX_PLAYERS));
    for player_idx in 0..gs.num_players.min(gameplay::MAX_PLAYERS) {
        let api_key = gs.player_profiles[player_idx].arrowcloud_api_key.trim();
        if api_key.is_empty() {
            continue;
        }
        let Some(payload) = arrowcloud_payload_for_player(gs, player_idx) else {
            continue;
        };
        let token = arrowcloud_next_submit_ui_token();
        arrowcloud_set_submit_ui_status(
            gameplay_side_for_player(gs, player_idx),
            payload.hash.as_str(),
            token,
            ArrowCloudSubmitUiStatus::Submitting,
        );
        jobs.push(ArrowCloudSubmitJob {
            side: gameplay_side_for_player(gs, player_idx),
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
    max_entries: usize,
}

#[derive(Debug, Clone)]
enum PlayerLeaderboardCacheValue {
    Ready(PlayerLeaderboardData),
    Error(String),
}

#[derive(Debug, Clone)]
struct PlayerLeaderboardCacheEntry {
    value: PlayerLeaderboardCacheValue,
    refreshed_at: Instant,
}

#[derive(Default)]
struct PlayerLeaderboardCacheState {
    by_key: HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheEntry>,
    in_flight: HashSet<PlayerLeaderboardCacheKey>,
}

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

const PLAYER_LEADERBOARD_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL: Duration = Duration::from_secs(10);
const ARROWCLOUD_LEADERBOARDS_BASE_URL: &str = "https://api.arrowcloud.dance";

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
enum F64OrString {
    F64(f64),
    String(String),
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

    let max_entries = max_entries.max(1).to_string();
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
        data: PlayerLeaderboardData { panes },
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
fn should_refresh_player_leaderboard_entry(entry: &PlayerLeaderboardCacheEntry) -> bool {
    let age = entry.refreshed_at.elapsed();
    match entry.value {
        PlayerLeaderboardCacheValue::Ready(_) => age >= PLAYER_LEADERBOARD_REFRESH_INTERVAL,
        PlayerLeaderboardCacheValue::Error(_) => age >= PLAYER_LEADERBOARD_ERROR_RETRY_INTERVAL,
    }
}

pub fn get_or_fetch_player_leaderboards_for_side(
    chart_hash: &str,
    side: profile::PlayerSide,
    max_entries: usize,
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
    let auto_username = side_profile.groovestats_username.trim().to_string();
    let should_auto_populate =
        auto_populate && auto_profile_id.is_some() && !auto_username.is_empty();

    let key = PlayerLeaderboardCacheKey {
        chart_hash: chart_hash.to_string(),
        api_key: gs_api_key.to_string(),
        arrowcloud_api_key,
        include_arrowcloud,
        show_ex_score: side_profile.show_ex_score,
        max_entries,
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
    let snapshot = {
        let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        let snapshot = if let Some(entry) = cache.by_key.get(&key) {
            cache_snapshot_from_entry(entry)
        } else {
            loading_player_leaderboard_snapshot()
        };

        if !cache.in_flight.contains(&key)
            && cache
                .by_key
                .get(&key)
                .is_none_or(should_refresh_player_leaderboard_entry)
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
                if key.include_arrowcloud {
                    Some(key.arrowcloud_api_key.as_str())
                } else {
                    None
                },
                key.show_ex_score,
                key.max_entries,
            );
            let refresh_finished_at = Instant::now();
            let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
            cache.in_flight.remove(&key);

            match fetched {
                Ok(fetched) => {
                    if should_auto_populate && let Some(profile_id) = auto_profile_id.as_deref() {
                        cache_gs_score_from_leaderboard(
                            profile_id,
                            auto_username.as_str(),
                            key.chart_hash.as_str(),
                            fetched.gs_entries.as_slice(),
                        );
                    }
                    cache.by_key.insert(
                        key,
                        PlayerLeaderboardCacheEntry {
                            value: PlayerLeaderboardCacheValue::Ready(fetched.data),
                            refreshed_at: refresh_finished_at,
                        },
                    );
                }
                Err(error) => {
                    if let Some(entry) = cache.by_key.get_mut(&key)
                        && matches!(entry.value, PlayerLeaderboardCacheValue::Ready(_))
                    {
                        // Keep stale data visible on refresh failures, but back off retries.
                        entry.refreshed_at = refresh_finished_at;
                    } else {
                        cache.by_key.insert(
                            key,
                            PlayerLeaderboardCacheEntry {
                                value: PlayerLeaderboardCacheValue::Error(error.to_string()),
                                refreshed_at: refresh_finished_at,
                            },
                        );
                    }
                }
            }
        });
    }

    Some(snapshot)
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
        info!(
            "GrooveStats lamp: hash={} score={:.4}% -> Quad lamp (W1 FC, no DP check needed)",
            chart_hash,
            score_percent * 100.0
        );
        return Some(1);
    }

    let comment = if let Some(c) = comment {
        c
    } else {
        info!(
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
        info!(
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
        info!(
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
        info!(
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
            info!(
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
            info!(
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
        info!(
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
            info!(
                "{} score import canceled after {idx}/{requested_charts} charts.",
                endpoint.display_name()
            );
            break;
        }
        wait_for_next_import_request(last_request_started_at);
        if should_cancel() {
            canceled = true;
            info!(
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
            info!(
                "{} bulk import progress for '{}': {done}/{requested_charts} charts (imported={}, missing={}, failed={})",
                endpoint.display_name(),
                username,
                imported_scores,
                missing_scores,
                failed_requests
            );
        }
        info!("{detail}");
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

    info!(
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
