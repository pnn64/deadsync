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
use serde::Deserialize;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

    // NoMines handling is not wired yet, so treat mines as enabled.
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
            gs.charts[player_idx].stats.total_steps,
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
            gs.charts[player_idx].stats.total_steps,
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
    show_ex_score: bool,
    max_entries: usize,
}

#[derive(Debug, Clone)]
enum PlayerLeaderboardCacheValue {
    Loading,
    Ready(PlayerLeaderboardData),
    Error(String),
}

#[derive(Default)]
struct PlayerLeaderboardCacheState {
    by_key: HashMap<PlayerLeaderboardCacheKey, PlayerLeaderboardCacheValue>,
}

static PLAYER_LEADERBOARD_CACHE: std::sync::LazyLock<Mutex<PlayerLeaderboardCacheState>> =
    std::sync::LazyLock::new(|| Mutex::new(PlayerLeaderboardCacheState::default()));

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LeaderboardsApiResponse {
    player1: Option<LeaderboardApiPlayer>,
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

#[derive(Deserialize, Debug)]
struct ApiResponse {
    player1: Option<Player1>,
}

#[derive(Deserialize, Debug)]
struct Player1 {
    #[serde(rename = "gsLeaderboard")]
    gs_leaderboard: Option<Vec<GrooveScore>>,
}

#[derive(Deserialize, Debug)]
struct GrooveScore {
    name: String,
    score: f64, // 0..10000
    /// Optional human-readable comment string (e.g., "189w, 33e, 2g, 1d, 3m, C690").
    /// This is generated by Simply Love as part of `GrooveStats` score submission
    /// and exposed via the `comments` field in `GrooveStats`' JSON.
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

fn fetch_player_leaderboards_internal(
    chart_hash: &str,
    api_key: &str,
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
    let Some(player) = decoded.player1 else {
        return Ok(FetchedPlayerLeaderboards {
            data: PlayerLeaderboardData { panes: Vec::new() },
            gs_entries: Vec::new(),
        });
    };
    let LeaderboardApiPlayer {
        is_ranked: _is_ranked,
        gs_leaderboard,
        ex_leaderboard,
        rpg,
        itl,
    } = player;

    let gs_entries = gs_leaderboard.clone();
    let mut panes = Vec::with_capacity(4);
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

    Ok(FetchedPlayerLeaderboards {
        data: PlayerLeaderboardData { panes },
        gs_entries,
    })
}

pub fn fetch_player_leaderboards(
    chart_hash: &str,
    api_key: &str,
    show_ex_score: bool,
    max_entries: usize,
) -> Result<PlayerLeaderboardData, Box<dyn Error + Send + Sync>> {
    fetch_player_leaderboards_internal(chart_hash, api_key, show_ex_score, max_entries)
        .map(|fetched| fetched.data)
}

#[inline(always)]
fn cache_snapshot_from_value(value: &PlayerLeaderboardCacheValue) -> CachedPlayerLeaderboardData {
    match value {
        PlayerLeaderboardCacheValue::Loading => CachedPlayerLeaderboardData {
            loading: true,
            data: None,
            error: None,
        },
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
    if side_profile.groovestats_api_key.trim().is_empty() {
        return None;
    }
    let auto_populate = cfg.auto_populate_gs_scores;
    let auto_profile_id = if auto_populate {
        profile::active_local_profile_id_for_side(side)
    } else {
        None
    };
    let auto_username = side_profile.groovestats_username.trim().to_string();
    let should_auto_populate =
        auto_populate && auto_profile_id.is_some() && !auto_username.is_empty();

    match network::get_status() {
        network::ConnectionStatus::Pending => {
            return Some(CachedPlayerLeaderboardData {
                loading: true,
                data: None,
                error: None,
            });
        }
        network::ConnectionStatus::Connected(services) if !services.get_scores => {
            return Some(CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: Some("Disabled".to_string()),
            });
        }
        network::ConnectionStatus::Error(error) => {
            return Some(CachedPlayerLeaderboardData {
                loading: false,
                data: None,
                error: Some(error),
            });
        }
        _ => {}
    }

    let key = PlayerLeaderboardCacheKey {
        chart_hash: chart_hash.to_string(),
        api_key: side_profile.groovestats_api_key.clone(),
        show_ex_score: side_profile.show_ex_score,
        max_entries,
    };

    let mut should_spawn = false;
    let snapshot = {
        let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
        if let Some(value) = cache.by_key.get(&key) {
            cache_snapshot_from_value(value)
        } else {
            cache
                .by_key
                .insert(key.clone(), PlayerLeaderboardCacheValue::Loading);
            should_spawn = true;
            CachedPlayerLeaderboardData {
                loading: true,
                data: None,
                error: None,
            }
        }
    };

    if should_spawn {
        std::thread::spawn(move || {
            let result = fetch_player_leaderboards_internal(
                &key.chart_hash,
                &key.api_key,
                key.show_ex_score,
                key.max_entries,
            )
            .map(|fetched| {
                if should_auto_populate && let Some(profile_id) = auto_profile_id.as_deref() {
                    cache_gs_score_from_leaderboard(
                        profile_id,
                        auto_username.as_str(),
                        key.chart_hash.as_str(),
                        fetched.gs_entries.as_slice(),
                    );
                }
                PlayerLeaderboardCacheValue::Ready(fetched.data)
            })
            .unwrap_or_else(|e| PlayerLeaderboardCacheValue::Error(e.to_string()));

            let mut cache = PLAYER_LEADERBOARD_CACHE.lock().unwrap();
            cache.by_key.insert(key, result);
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

pub fn fetch_and_store_grade(
    profile_id: String,
    profile: Profile,
    chart_hash: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if profile.groovestats_api_key.is_empty() || profile.groovestats_username.is_empty() {
        return Err("GrooveStats API key or username is not set in profile.ini.".into());
    }

    info!(
        "Requesting scores for '{}' on chart '{}'...",
        profile.groovestats_username, chart_hash
    );

    let agent = network::get_agent();
    let api_url = network::groovestats_player_leaderboards_url();
    let response = agent
        .get(&api_url)
        .header("x-api-key-player-1", &profile.groovestats_api_key)
        .query("chartHashP1", &chart_hash)
        .call()?;

    if response.status() != 200 {
        return Err(format!("API returned status {}", response.status()).into());
    }

    let api_response: ApiResponse = response.into_body().read_json()?;

    let player_score = api_response
        .player1
        .and_then(|p1| p1.gs_leaderboard)
        .and_then(|scores| {
            scores
                .into_iter()
                .find(|s| s.name.eq_ignore_ascii_case(&profile.groovestats_username))
        });

    if let Some(score_data) = player_score {
        let cached_score = cached_score_from_gs(
            score_data.score,
            score_data.comments.as_deref(),
            &chart_hash,
            false,
        );
        cache_gs_score_for_profile(
            &profile_id,
            &chart_hash,
            cached_score,
            &profile.groovestats_username,
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
