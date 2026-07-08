use crate::CachedItlScore;
use bincode::{Decode, Encode};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, LazyLock, Mutex};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ItlFileData {
    #[serde(rename = "pathMap", default)]
    pub path_map: HashMap<String, String>,
    #[serde(rename = "hashMap", default)]
    pub hash_map: HashMap<String, ItlHashEntry>,
    #[serde(default)]
    pub points: Vec<u32>,
    #[serde(rename = "pointsSingle", default)]
    pub points_single: Vec<u32>,
    #[serde(rename = "pointsDouble", default)]
    pub points_double: Vec<u32>,
    #[serde(rename = "unlockFolders", default)]
    pub unlock_folders: HashMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub struct OnlineItlSelfScoreKey {
    pub chart_hash: String,
    pub api_key: String,
}

/// Borrowed view of [`OnlineItlSelfScoreKey`] for allocation-free cache probes.
/// Hashes identically to the owned key, so it can look up entries in the
/// hashbrown caches without building an owned key.
#[derive(Hash)]
pub struct OnlineItlSelfScoreKeyRef<'a> {
    pub chart_hash: &'a str,
    pub api_key: &'a str,
}

impl hashbrown::Equivalent<OnlineItlSelfScoreKey> for OnlineItlSelfScoreKeyRef<'_> {
    fn equivalent(&self, key: &OnlineItlSelfScoreKey) -> bool {
        self.chart_hash == key.chart_hash && self.api_key == key.api_key
    }
}

pub type OnlineItlSelfCacheMap = hashbrown::HashMap<OnlineItlSelfScoreKey, u32>;
pub type OnlineItlSelfIndexMap = HashMap<OnlineItlSelfScoreKey, u32>;

static ONLINE_ITL_SELF_SCORE_CACHE: LazyLock<Mutex<OnlineItlSelfCacheState>> =
    LazyLock::new(|| Mutex::new(OnlineItlSelfCacheState::default()));
static ONLINE_ITL_SELF_SCORE_GENERATION: AtomicU64 = AtomicU64::new(1);
static ONLINE_ITL_SELF_RANK_CACHE: LazyLock<Mutex<OnlineItlSelfCacheState>> =
    LazyLock::new(|| Mutex::new(OnlineItlSelfCacheState::default()));
static ONLINE_ITL_OVERALL_RANK_CACHE: LazyLock<Mutex<OnlineItlOverallRankCacheState>> =
    LazyLock::new(|| Mutex::new(OnlineItlOverallRankCacheState::default()));
static EMPTY_ONLINE_ITL_OVERALL_RANKS: LazyLock<Arc<HashMap<String, u32>>> =
    LazyLock::new(|| Arc::new(HashMap::new()));
static ITL_SCORE_CACHE: LazyLock<Mutex<ItlScoreCacheState>> =
    LazyLock::new(|| Mutex::new(ItlScoreCacheState::default()));

#[derive(Default)]
pub struct OnlineItlSelfCacheState {
    session_by_key: OnlineItlSelfCacheMap,
    loaded_profiles: HashMap<String, OnlineItlSelfCacheMap>,
}

pub struct OnlineItlSelfCacheUpdate {
    pub changed: bool,
    pub profile_snapshot: Option<(String, OnlineItlSelfCacheMap)>,
}

impl OnlineItlSelfCacheState {
    #[inline(always)]
    pub fn profile_loaded(&self, profile_id: &str) -> bool {
        self.loaded_profiles.contains_key(profile_id)
    }

    pub fn insert_loaded_profile(&mut self, profile_id: &str, by_key: OnlineItlSelfIndexMap) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_insert_with(|| by_key.into_iter().collect());
    }

    pub fn set_value(
        &mut self,
        profile_id: Option<&str>,
        api_key: &str,
        chart_hash: &str,
        value: Option<u32>,
    ) -> OnlineItlSelfCacheUpdate {
        let api_key = api_key.trim();
        let chart_hash = chart_hash.trim();
        if api_key.is_empty() || chart_hash.is_empty() {
            return OnlineItlSelfCacheUpdate {
                changed: false,
                profile_snapshot: None,
            };
        }

        let key = OnlineItlSelfScoreKey {
            chart_hash: chart_hash.to_string(),
            api_key: api_key.to_string(),
        };
        let session_changed = if let Some(value) = value {
            self.session_by_key.insert(key.clone(), value) != Some(value)
        } else {
            self.session_by_key.remove(&key).is_some()
        };

        let Some(profile_id) = profile_id.map(str::trim).filter(|id| !id.is_empty()) else {
            return OnlineItlSelfCacheUpdate {
                changed: session_changed,
                profile_snapshot: None,
            };
        };
        let Some(profile_values) = self.loaded_profiles.get_mut(profile_id) else {
            return OnlineItlSelfCacheUpdate {
                changed: session_changed,
                profile_snapshot: None,
            };
        };
        let profile_changed = if let Some(value) = value {
            profile_values.insert(key, value) != Some(value)
        } else {
            profile_values.remove(&key).is_some()
        };

        OnlineItlSelfCacheUpdate {
            changed: session_changed || profile_changed,
            profile_snapshot: profile_changed
                .then(|| (profile_id.to_string(), profile_values.clone())),
        }
    }

    pub fn get_value(
        &self,
        chart_hash: &str,
        profile_id: Option<&str>,
        api_key: &str,
    ) -> Option<u32> {
        let chart_hash = chart_hash.trim();
        let api_key = api_key.trim();
        if chart_hash.is_empty() || api_key.is_empty() {
            return None;
        }
        let kref = OnlineItlSelfScoreKeyRef {
            chart_hash,
            api_key,
        };
        profile_id
            .and_then(|profile_id| self.loaded_profiles.get(profile_id))
            .and_then(|values| values.get(&kref).copied())
            .or_else(|| self.session_by_key.get(&kref).copied())
    }

    pub fn values_by_chart_for_api(
        &self,
        profile_id: Option<&str>,
        api_key: &str,
    ) -> HashMap<String, u32> {
        let api_key = api_key.trim();
        if api_key.is_empty() {
            return HashMap::new();
        }
        let loaded_count = profile_id
            .and_then(|profile_id| self.loaded_profiles.get(profile_id))
            .map_or(0, |scores| scores.len());
        let mut by_chart = HashMap::with_capacity(loaded_count + self.session_by_key.len());
        if let Some(profile_id) = profile_id
            && let Some(values) = self.loaded_profiles.get(profile_id)
        {
            for (key, value) in values {
                if key.api_key == api_key {
                    by_chart.insert(key.chart_hash.clone(), *value);
                }
            }
        }
        for (key, value) in &self.session_by_key {
            if key.api_key == api_key {
                by_chart.insert(key.chart_hash.clone(), *value);
            }
        }
        by_chart
    }
}

pub fn online_itl_self_score_profile_loaded(profile_id: &str) -> bool {
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_loaded(profile_id)
}

pub fn insert_online_itl_self_score_profile(profile_id: &str, by_key: OnlineItlSelfIndexMap) {
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, by_key);
}

pub fn runtime_ensure_online_itl_self_score_profile_loaded<L>(profile_id: &str, mut load_profile: L)
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    let profile_id = profile_id.trim();
    if profile_id.is_empty() || online_itl_self_score_profile_loaded(profile_id) {
        return;
    }
    insert_online_itl_self_score_profile(profile_id, load_profile(profile_id));
}

pub fn set_online_itl_self_score(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    score: Option<u32>,
) -> OnlineItlSelfCacheUpdate {
    let update = ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .set_value(profile_id, api_key, chart_hash, score);
    if update.changed {
        ONLINE_ITL_SELF_SCORE_GENERATION.fetch_add(1, AtomicOrdering::Relaxed);
    }
    update
}

pub fn runtime_set_online_itl_self_score<L, S>(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    score: Option<u32>,
    mut load_profile: L,
    mut save_profile: S,
) where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
    S: FnMut(&str, &OnlineItlSelfCacheMap),
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, &mut load_profile);
    }

    let update = set_online_itl_self_score(profile_id, api_key, chart_hash, score);
    if let Some((profile_id, by_key)) = update.profile_snapshot {
        save_profile(profile_id.as_str(), &by_key);
    }
}

pub fn online_itl_self_score_generation() -> u64 {
    ONLINE_ITL_SELF_SCORE_GENERATION.load(AtomicOrdering::Relaxed)
}

pub fn online_itl_self_scores_by_chart_for_api(
    profile_id: Option<&str>,
    api_key: &str,
) -> HashMap<String, u32> {
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .values_by_chart_for_api(profile_id, api_key)
}

pub fn get_online_itl_self_score(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    ONLINE_ITL_SELF_SCORE_CACHE
        .lock()
        .unwrap()
        .get_value(chart_hash, profile_id, api_key)
}

pub fn online_itl_self_rank_profile_loaded(profile_id: &str) -> bool {
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .profile_loaded(profile_id)
}

pub fn insert_online_itl_self_rank_profile(profile_id: &str, by_key: OnlineItlSelfIndexMap) {
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, by_key);
}

pub fn runtime_ensure_online_itl_self_rank_profile_loaded<L>(profile_id: &str, mut load_profile: L)
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    let profile_id = profile_id.trim();
    if profile_id.is_empty() || online_itl_self_rank_profile_loaded(profile_id) {
        return;
    }
    insert_online_itl_self_rank_profile(profile_id, load_profile(profile_id));
}

pub fn set_online_itl_self_rank(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    rank: Option<u32>,
) -> OnlineItlSelfCacheUpdate {
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .set_value(profile_id, api_key, chart_hash, rank)
}

pub fn runtime_set_online_itl_self_rank<L, S>(
    profile_id: Option<&str>,
    api_key: &str,
    chart_hash: &str,
    rank: Option<u32>,
    mut load_profile: L,
    mut save_profile: S,
) where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
    S: FnMut(&str, &OnlineItlSelfCacheMap),
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_rank_profile_loaded(profile_id, &mut load_profile);
    }

    let update = set_online_itl_self_rank(profile_id, api_key, chart_hash, rank);
    if let Some((profile_id, by_key)) = update.profile_snapshot {
        save_profile(profile_id.as_str(), &by_key);
    }
}

pub fn get_online_itl_self_rank(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    ONLINE_ITL_SELF_RANK_CACHE
        .lock()
        .unwrap()
        .get_value(chart_hash, profile_id, api_key)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnlineItlOverallRankCacheKey {
    pub api_key: String,
    pub profile_id: Option<String>,
    pub song_cache_generation: u64,
    pub self_score_generation: u64,
}

#[derive(Clone)]
struct OnlineItlOverallRankCacheEntry {
    key: OnlineItlOverallRankCacheKey,
    ranks: Arc<HashMap<String, u32>>,
}

#[derive(Default)]
struct OnlineItlOverallRankCacheState {
    p1: Option<OnlineItlOverallRankCacheEntry>,
    p2: Option<OnlineItlOverallRankCacheEntry>,
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side(
    state: &OnlineItlOverallRankCacheState,
    side_idx: usize,
) -> Option<&OnlineItlOverallRankCacheEntry> {
    match side_idx {
        1 => state.p2.as_ref(),
        _ => state.p1.as_ref(),
    }
}

#[inline(always)]
fn online_itl_overall_rank_entry_for_side_mut(
    state: &mut OnlineItlOverallRankCacheState,
    side_idx: usize,
) -> &mut Option<OnlineItlOverallRankCacheEntry> {
    match side_idx {
        1 => &mut state.p2,
        _ => &mut state.p1,
    }
}

pub fn empty_online_itl_overall_ranks() -> Arc<HashMap<String, u32>> {
    EMPTY_ONLINE_ITL_OVERALL_RANKS.clone()
}

pub fn cached_online_itl_overall_ranks_for_side(
    side_idx: usize,
    key: &OnlineItlOverallRankCacheKey,
) -> Option<Arc<HashMap<String, u32>>> {
    let cache = ONLINE_ITL_OVERALL_RANK_CACHE.lock().unwrap();
    online_itl_overall_rank_entry_for_side(&cache, side_idx)
        .filter(|entry| entry.key == *key)
        .map(|entry| entry.ranks.clone())
}

pub fn store_online_itl_overall_ranks_for_side(
    side_idx: usize,
    key: OnlineItlOverallRankCacheKey,
    ranks: Arc<HashMap<String, u32>>,
) -> Arc<HashMap<String, u32>> {
    let mut cache = ONLINE_ITL_OVERALL_RANK_CACHE.lock().unwrap();
    *online_itl_overall_rank_entry_for_side_mut(&mut cache, side_idx) =
        Some(OnlineItlOverallRankCacheEntry {
            key,
            ranks: ranks.clone(),
        });
    ranks
}

pub fn runtime_online_itl_overall_ranks_for_side<L>(
    side_idx: usize,
    side_joined: bool,
    api_key: &str,
    profile_id: Option<&str>,
    song_cache_generation: u64,
    song_cache: &[deadsync_chart::SongPack],
    mut load_profile: L,
) -> Arc<HashMap<String, u32>>
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    if !side_joined {
        return empty_online_itl_overall_ranks();
    }

    let api_key = api_key.trim();
    if api_key.is_empty() {
        return empty_online_itl_overall_ranks();
    }

    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, &mut load_profile);
    }

    let self_score_generation = online_itl_self_score_generation();
    let key = OnlineItlOverallRankCacheKey {
        api_key: api_key.to_string(),
        profile_id: profile_id.map(str::to_string),
        song_cache_generation,
        self_score_generation,
    };
    if let Some(ranks) = cached_online_itl_overall_ranks_for_side(side_idx, &key) {
        return ranks;
    }

    let by_chart_score = online_itl_self_scores_by_chart_for_api(profile_id, api_key);
    let ranks = Arc::new(itl_overall_ranks_from_song_cache(
        song_cache,
        &by_chart_score,
    ));
    store_online_itl_overall_ranks_for_side(side_idx, key, ranks)
}

#[derive(Debug)]
pub enum OnlineItlSelfIndexWriteError {
    CreateDir {
        dir: PathBuf,
        error: std::io::Error,
    },
    Encode {
        path: PathBuf,
    },
    WriteTemp {
        path: PathBuf,
        error: std::io::Error,
    },
    Commit {
        path: PathBuf,
        error: std::io::Error,
    },
}

pub fn load_online_itl_self_index_file(path: &Path) -> Option<OnlineItlSelfIndexMap> {
    let bytes = fs::read(path).ok()?;
    let (by_key, _) =
        bincode::decode_from_slice::<OnlineItlSelfIndexMap, _>(&bytes, bincode::config::standard())
            .ok()?;
    Some(by_key)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineItlSelfIndexKind {
    Score,
    Rank,
}

pub fn online_itl_self_index_path(profile_dir: &Path, kind: OnlineItlSelfIndexKind) -> PathBuf {
    let file_name = match kind {
        OnlineItlSelfIndexKind::Score => "itl_self.bin",
        OnlineItlSelfIndexKind::Rank => "itl_rank.bin",
    };
    profile_dir.join("scores").join("gs").join(file_name)
}

pub fn load_online_itl_self_index_for_profile_dir(
    profile_dir: &Path,
    kind: OnlineItlSelfIndexKind,
) -> OnlineItlSelfIndexMap {
    load_online_itl_self_index_file(&online_itl_self_index_path(profile_dir, kind))
        .unwrap_or_default()
}

pub fn save_online_itl_self_index_for_profile_dir(
    profile_dir: &Path,
    kind: OnlineItlSelfIndexKind,
    by_key: &OnlineItlSelfCacheMap,
) -> Result<PathBuf, OnlineItlSelfIndexWriteError> {
    let path = online_itl_self_index_path(profile_dir, kind);
    save_online_itl_self_index_file(&path, by_key)?;
    Ok(path)
}

pub fn save_online_itl_self_index_file(
    path: &Path,
    by_key: &OnlineItlSelfCacheMap,
) -> Result<(), OnlineItlSelfIndexWriteError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| OnlineItlSelfIndexWriteError::CreateDir {
        dir: parent.to_path_buf(),
        error,
    })?;

    let std_by_key: OnlineItlSelfIndexMap = by_key
        .iter()
        .map(|(key, value)| (key.clone(), *value))
        .collect();
    let buf = bincode::encode_to_vec(&std_by_key, bincode::config::standard()).map_err(|_| {
        OnlineItlSelfIndexWriteError::Encode {
            path: path.to_path_buf(),
        }
    })?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, buf).map_err(|error| OnlineItlSelfIndexWriteError::WriteTemp {
        path: tmp_path.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(OnlineItlSelfIndexWriteError::Commit {
            path: path.to_path_buf(),
            error,
        });
    }
    Ok(())
}

#[derive(Default)]
pub struct ItlScoreCacheState {
    loaded_profiles: HashMap<String, ItlFileData>,
}

impl ItlScoreCacheState {
    #[inline(always)]
    pub fn profile_loaded(&self, profile_id: &str) -> bool {
        self.loaded_profiles.contains_key(profile_id)
    }

    pub fn insert_loaded_profile(&mut self, profile_id: &str, data: ItlFileData) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_insert(data);
    }

    pub fn set_profile_data(&mut self, profile_id: &str, data: ItlFileData) {
        self.loaded_profiles.insert(profile_id.to_string(), data);
    }

    pub fn mark_unlock_folders<'a, I>(&mut self, profile_id: &str, folders: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let data = self
            .loaded_profiles
            .entry(profile_id.to_string())
            .or_default();
        itl_mark_unlock_folders(data, folders);
    }

    pub fn chart_score(&self, profile_id: &str, chart_hash: &str) -> Option<CachedItlScore> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|data| data.hash_map.get(chart_hash))
            .map(itl_score_from_entry)
    }

    pub fn song_score(
        &self,
        profile_id: &str,
        song: &deadsync_chart::SongData,
    ) -> Option<CachedItlScore> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|data| itl_score_for_song(song, data))
    }

    pub fn song_folder_unlocked(&self, profile_id: &str, song_folder: &str) -> bool {
        self.loaded_profiles
            .get(profile_id)
            .map(|data| itl_song_folder_unlocked(data, song_folder))
            .unwrap_or(false)
    }

    pub fn chart_no_cmod_for_song(
        &self,
        profile_id: &str,
        song_dir: Option<&str>,
        group_name: Option<&str>,
        chart_hash: &str,
        subtitle: &str,
    ) -> Option<bool> {
        let data = self.loaded_profiles.get(profile_id)?;
        if !itl_song_matches_context(song_dir, group_name, data) {
            return Some(false);
        }
        let prev = data.hash_map.get(chart_hash);
        Some(itl_chart_no_cmod(subtitle, prev))
    }
}

pub fn itl_score_profile_loaded(profile_id: &str) -> bool {
    ITL_SCORE_CACHE.lock().unwrap().profile_loaded(profile_id)
}

pub fn insert_itl_score_profile(profile_id: &str, data: ItlFileData) {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, data);
}

pub fn set_itl_score_profile(profile_id: &str, data: ItlFileData) {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .set_profile_data(profile_id, data);
}

pub fn runtime_ensure_itl_score_profile_loaded<L>(profile_id: &str, mut load_profile: L)
where
    L: FnMut(&str) -> ItlFileData,
{
    let profile_id = profile_id.trim();
    if profile_id.is_empty() || itl_score_profile_loaded(profile_id) {
        return;
    }

    let mut data = load_profile(profile_id);
    itl_rebuild_song_ranks(&mut data);
    insert_itl_score_profile(profile_id, data);
}

pub fn runtime_set_itl_score_file(profile_id: &str, data: ItlFileData) {
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return;
    }
    set_itl_score_profile(profile_id, data);
}

pub fn runtime_import_itl_json<W>(profile_id: &str, json_text: &str, mut write_profile: W) -> usize
where
    W: FnMut(&str, &ItlFileData),
{
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return 0;
    }
    let Some(data) = itl_data_from_json(json_text) else {
        return 0;
    };
    let count = data.hash_map.len();
    write_profile(profile_id, &data);
    count
}

pub fn runtime_update_itl_unlock_folders<'a, I, R, W>(
    profile_id: &str,
    folders: I,
    mut read_profile: R,
    mut write_profile: W,
) -> bool
where
    I: IntoIterator<Item = &'a str>,
    R: FnMut(&str) -> ItlFileData,
    W: FnMut(&str, &ItlFileData),
{
    let profile_id = profile_id.trim();
    if profile_id.is_empty() {
        return false;
    }
    let folders: Vec<&str> = folders.into_iter().collect();
    if folders.is_empty() {
        return false;
    }

    let mut data = read_profile(profile_id);
    if !itl_mark_unlock_folders(&mut data, folders) {
        return false;
    }
    write_profile(profile_id, &data);
    set_itl_score_profile(profile_id, data);
    true
}

pub fn mark_itl_unlock_folders<'a, I>(profile_id: &str, folders: I)
where
    I: IntoIterator<Item = &'a str>,
{
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .mark_unlock_folders(profile_id, folders);
}

pub fn cached_itl_chart_score(profile_id: &str, chart_hash: &str) -> Option<CachedItlScore> {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .chart_score(profile_id, chart_hash)
}

pub fn runtime_cached_itl_chart_score<L>(
    profile_id: Option<&str>,
    chart_hash: &str,
    mut load_profile: L,
) -> Option<CachedItlScore>
where
    L: FnMut(&str) -> ItlFileData,
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty())?;
    runtime_ensure_itl_score_profile_loaded(profile_id, &mut load_profile);
    cached_itl_chart_score(profile_id, chart_hash)
}

pub fn cached_itl_song_score(
    profile_id: &str,
    song: &deadsync_chart::SongData,
) -> Option<CachedItlScore> {
    ITL_SCORE_CACHE.lock().unwrap().song_score(profile_id, song)
}

pub fn runtime_cached_itl_song_score<L>(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
    mut load_profile: L,
) -> Option<CachedItlScore>
where
    L: FnMut(&str) -> ItlFileData,
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty())?;
    runtime_ensure_itl_score_profile_loaded(profile_id, &mut load_profile);
    cached_itl_song_score(profile_id, song)
}

pub fn runtime_cached_itl_song_score_assume_loaded(
    song: &deadsync_chart::SongData,
    profile_id: Option<&str>,
) -> Option<CachedItlScore> {
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty())?;
    cached_itl_song_score(profile_id, song)
}

pub fn cached_itl_song_folder_unlocked(profile_id: &str, song_folder: &str) -> bool {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .song_folder_unlocked(profile_id, song_folder)
}

pub fn runtime_cached_itl_song_folder_unlocked<L>(
    song_folder: &str,
    profile_id: Option<&str>,
    mut load_profile: L,
) -> bool
where
    L: FnMut(&str) -> ItlFileData,
{
    let Some(profile_id) = profile_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return false;
    };
    runtime_ensure_itl_score_profile_loaded(profile_id, &mut load_profile);
    cached_itl_song_folder_unlocked(profile_id, song_folder)
}

pub fn cached_itl_chart_no_cmod_for_song(
    profile_id: &str,
    song_dir: Option<&str>,
    group_name: Option<&str>,
    chart_hash: &str,
    subtitle: &str,
) -> Option<bool> {
    ITL_SCORE_CACHE
        .lock()
        .unwrap()
        .chart_no_cmod_for_song(profile_id, song_dir, group_name, chart_hash, subtitle)
}

pub fn runtime_cached_online_itl_self_score<L>(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
    mut load_profile: L,
) -> Option<u32>
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, &mut load_profile);
    }
    runtime_cached_online_itl_self_score_assume_loaded(chart_hash, profile_id, api_key)
}

pub fn runtime_cached_online_itl_self_score_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return None;
    }
    get_online_itl_self_score(chart_hash, profile_id, api_key)
}

pub fn runtime_cached_online_itl_self_rank<L>(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
    mut load_profile: L,
) -> Option<u32>
where
    L: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    let profile_id = profile_id.map(str::trim).filter(|id| !id.is_empty());
    if let Some(profile_id) = profile_id {
        runtime_ensure_online_itl_self_rank_profile_loaded(profile_id, &mut load_profile);
    }
    runtime_cached_online_itl_self_rank_assume_loaded(chart_hash, profile_id, api_key)
}

pub fn runtime_cached_online_itl_self_rank_assume_loaded(
    chart_hash: &str,
    profile_id: Option<&str>,
    api_key: &str,
) -> Option<u32> {
    let chart_hash = chart_hash.trim();
    let api_key = api_key.trim();
    if chart_hash.is_empty() || api_key.is_empty() {
        return None;
    }
    get_online_itl_self_rank(chart_hash, profile_id, api_key)
}

pub fn runtime_ensure_itl_wheel_caches_loaded<I, S, R>(
    profile_id: &str,
    load_itl_profile: I,
    load_self_score_profile: S,
    load_self_rank_profile: R,
) where
    I: FnMut(&str) -> ItlFileData,
    S: FnMut(&str) -> OnlineItlSelfIndexMap,
    R: FnMut(&str) -> OnlineItlSelfIndexMap,
{
    runtime_ensure_itl_score_profile_loaded(profile_id, load_itl_profile);
    runtime_ensure_online_itl_self_score_profile_loaded(profile_id, load_self_score_profile);
    runtime_ensure_online_itl_self_rank_profile_loaded(profile_id, load_self_rank_profile);
}

#[derive(Debug)]
pub enum ItlFileReadError {
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        error: serde_json::Error,
    },
}

#[derive(Debug)]
pub enum ItlFileWriteError {
    CreateDir {
        dir: PathBuf,
        error: std::io::Error,
    },
    Encode,
    WriteTemp {
        path: PathBuf,
        error: std::io::Error,
    },
    Commit {
        path: PathBuf,
        error: std::io::Error,
    },
}

pub fn read_itl_file_from_path(path: &Path) -> Result<ItlFileData, ItlFileReadError> {
    let text = fs::read_to_string(path).map_err(|error| ItlFileReadError::Read {
        path: path.to_path_buf(),
        error,
    })?;
    serde_json::from_str(text.as_str()).map_err(|error| ItlFileReadError::Parse {
        path: path.to_path_buf(),
        error,
    })
}

pub const ITL_PROFILE_FILE_NAME: &str = "ITL2026.json";

pub fn itl_profile_file_path(profile_dir: &Path) -> PathBuf {
    profile_dir.join(ITL_PROFILE_FILE_NAME)
}

pub fn read_itl_file_for_profile_dir(profile_dir: &Path) -> Result<ItlFileData, ItlFileReadError> {
    read_itl_file_from_path(&itl_profile_file_path(profile_dir))
}

pub fn read_itl_file_or_default_for_profile_dir(
    profile_dir: &Path,
) -> Result<ItlFileData, ItlFileReadError> {
    match read_itl_file_for_profile_dir(profile_dir) {
        Ok(data) => Ok(data),
        Err(ItlFileReadError::Read { .. }) => Ok(ItlFileData::default()),
        Err(error) => Err(error),
    }
}

pub fn write_itl_file_for_profile_dir(
    profile_dir: &Path,
    data: &ItlFileData,
) -> Result<PathBuf, ItlFileWriteError> {
    let path = itl_profile_file_path(profile_dir);
    write_itl_file_to_path(&path, data)?;
    Ok(path)
}

pub fn write_itl_file_to_path(path: &Path, data: &ItlFileData) -> Result<(), ItlFileWriteError> {
    if data.is_empty() {
        return Ok(());
    }
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| ItlFileWriteError::CreateDir {
        dir: parent.to_path_buf(),
        error,
    })?;
    let text = serde_json::to_string(data).map_err(|_| ItlFileWriteError::Encode)?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|error| ItlFileWriteError::WriteTemp {
        path: tmp.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(ItlFileWriteError::Commit {
            path: path.to_path_buf(),
            error,
        });
    }
    Ok(())
}

impl ItlFileData {
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.path_map.is_empty() && self.hash_map.is_empty() && self.unlock_folders.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItlHashEntry {
    #[serde(default)]
    pub judgments: ItlJudgments,
    #[serde(default, deserialize_with = "deserialize_itl_ex")]
    pub ex: u32,
    #[serde(rename = "clearType", default)]
    pub clear_type: u8,
    #[serde(default)]
    pub points: u32,
    #[serde(rename = "usedCmod", default)]
    pub used_cmod: bool,
    #[serde(default)]
    pub date: String,
    #[serde(rename = "noCmod", default)]
    pub no_cmod: bool,
    #[serde(rename = "passingPoints", default)]
    pub passing_points: u32,
    #[serde(rename = "maxScoringPoints", default)]
    pub max_scoring_points: u32,
    #[serde(rename = "maxPoints", default)]
    pub max_points: u32,
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(rename = "stepsType", default)]
    pub steps_type: String,
    #[serde(default)]
    pub passes: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ItlJudgments {
    #[serde(rename = "W0", default)]
    pub w0: u32,
    #[serde(rename = "W1", default)]
    pub w1: u32,
    #[serde(rename = "W2", default)]
    pub w2: u32,
    #[serde(rename = "W3", default)]
    pub w3: u32,
    #[serde(rename = "W4", default)]
    pub w4: u32,
    #[serde(rename = "W5", default)]
    pub w5: u32,
    #[serde(rename = "Miss", default)]
    pub miss: u32,
    #[serde(rename = "totalSteps", default)]
    pub total_steps: u32,
    #[serde(rename = "Holds", default)]
    pub holds: u32,
    #[serde(rename = "totalHolds", default)]
    pub total_holds: u32,
    #[serde(rename = "Mines", default)]
    pub mines: u32,
    #[serde(rename = "totalMines", default)]
    pub total_mines: u32,
    #[serde(rename = "Rolls", default)]
    pub rolls: u32,
    #[serde(rename = "totalRolls", default)]
    pub total_rolls: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItlJudgmentCountsInput {
    pub fantastic_plus: u32,
    pub fantastic: u32,
    pub excellent: u32,
    pub great: u32,
    pub decent: u32,
    pub way_off: u32,
    pub miss: u32,
    pub total_steps: u32,
    pub holds_held: u32,
    pub total_holds: u32,
    pub mines_hit: u32,
    pub total_mines: u32,
    pub rolls_held: u32,
    pub total_rolls: u32,
}

pub fn itl_judgments_from_counts(input: ItlJudgmentCountsInput) -> ItlJudgments {
    ItlJudgments {
        w0: input.fantastic_plus,
        w1: input.fantastic,
        w2: input.excellent,
        w3: input.great,
        w4: input.decent,
        w5: input.way_off,
        miss: input.miss,
        total_steps: input.total_steps,
        holds: input.holds_held,
        total_holds: input.total_holds,
        mines: input.mines_hit,
        total_mines: input.total_mines,
        rolls: input.rolls_held,
        total_rolls: input.total_rolls,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ItlScoreCalcInput<'a> {
    pub notes: &'a [deadsync_rules::note::Note],
    pub note_times: &'a [deadsync_core::song_time::SongTimeNs],
    pub hold_end_times: &'a [Option<deadsync_core::song_time::SongTimeNs>],
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub fail_time: Option<deadsync_core::song_time::SongTimeNs>,
}

pub fn itl_ex_score_percent(input: ItlScoreCalcInput<'_>) -> f64 {
    deadsync_rules::judgment::calculate_ex_score_from_notes(
        input.notes,
        input.note_times,
        input.hold_end_times,
        input.total_steps,
        input.holds_total,
        input.rolls_total,
        input.mines_total,
        input.fail_time,
        false,
    )
}

pub fn itl_current_score_hundredths(input: ItlScoreCalcInput<'_>) -> u32 {
    ex_hundredths(itl_ex_score_percent(input))
}

pub fn itl_timing_windows_all_enabled(disabled_windows: &[bool]) -> bool {
    disabled_windows.iter().all(|disabled| !*disabled)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItlPointTotals {
    pub ranking_points: u32,
    pub song_points: u32,
    pub ex_points: u32,
    pub total_points: u32,
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

/// Parses external Simply Love/ITGmania ITL JSON text into DeadSync's ITL
/// cache schema. Empty files and malformed text return `None`.
pub fn itl_data_from_json(json_text: &str) -> Option<ItlFileData> {
    let data: ItlFileData = serde_json::from_str(json_text).ok()?;
    if data.is_empty() {
        return None;
    }
    Some(data)
}

/// True when `pack_dir` matches the SL-style pattern `ITL Online <year> Unlocks`
/// (case-insensitive, any 4-digit year).
pub fn is_itl_unlocks_pack(pack_dir: &str) -> bool {
    const PREFIX: &[u8] = b"itl online ";
    const SUFFIX: &[u8] = b" unlocks";
    let bytes = pack_dir.trim().as_bytes();
    if bytes.len() != PREFIX.len() + 4 + SUFFIX.len() {
        return false;
    }
    let (prefix, rest) = bytes.split_at(PREFIX.len());
    let (year, suffix) = rest.split_at(4);
    prefix.eq_ignore_ascii_case(PREFIX)
        && suffix.eq_ignore_ascii_case(SUFFIX)
        && year.iter().all(u8::is_ascii_digit)
}

#[inline(always)]
pub fn itl_group_name_matches(group_name: &str) -> bool {
    let group = group_name.to_ascii_lowercase();
    group.contains("itl online 2026") || group.contains("itl 2026")
}

pub fn itl_song_matches(
    song_dir: Option<&str>,
    group_name: Option<&str>,
    data: &ItlFileData,
) -> bool {
    if song_dir.is_some_and(|dir| data.path_map.contains_key(dir)) {
        return true;
    }
    group_name.is_some_and(itl_group_name_matches)
}

pub fn itl_song_dir(song: &deadsync_chart::SongData) -> Option<String> {
    song.simfile_path
        .parent()
        .map(|dir| dir.to_string_lossy().into_owned())
}

pub fn itl_song_matches_context(
    song_dir: Option<&str>,
    group_name: Option<&str>,
    data: &ItlFileData,
) -> bool {
    itl_song_matches(song_dir, None, data) || itl_song_matches(None, group_name, data)
}

fn itl_entry_for_song<'a>(
    song: &deadsync_chart::SongData,
    data: &'a ItlFileData,
) -> Option<&'a ItlHashEntry> {
    let song_dir = song.simfile_path.parent()?.to_string_lossy();
    let chart_hash = data.path_map.get(song_dir.as_ref())?;
    data.hash_map.get(chart_hash)
}

pub fn itl_score_for_song(
    song: &deadsync_chart::SongData,
    data: &ItlFileData,
) -> Option<CachedItlScore> {
    itl_entry_for_song(song, data).map(itl_score_from_entry)
}

pub fn itl_chart_no_cmod(subtitle: &str, prev: Option<&ItlHashEntry>) -> bool {
    prev.map_or_else(
        || subtitle.to_ascii_lowercase().contains("no cmod"),
        |data| data.no_cmod,
    )
}

pub fn itl_event_name_from_group(group_name: Option<&str>) -> String {
    group_name
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "ITL Online 2026".to_string())
}

pub fn itl_steps_type_from_chart_type(chart_type: &str) -> &'static str {
    if chart_type.to_ascii_lowercase().contains("double") {
        "double"
    } else {
        "single"
    }
}

#[inline(always)]
pub fn itl_song_folder_unlocked(data: &ItlFileData, song_folder: &str) -> bool {
    data.unlock_folders
        .get(song_folder)
        .copied()
        .unwrap_or(false)
}

pub fn itl_mark_unlock_folders<'a, I>(data: &mut ItlFileData, folders: I) -> bool
where
    I: IntoIterator<Item = &'a str>,
{
    let mut changed = false;
    for folder in folders {
        let folder = folder.trim();
        if !folder.is_empty() {
            changed |= data.unlock_folders.insert(folder.to_string(), true) != Some(true);
        }
    }
    changed
}

#[inline(always)]
pub fn ex_hundredths(ex_percent: f64) -> u32 {
    let ex = if ex_percent.is_finite() {
        ex_percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    (ex * 100.0).round() as u32
}

pub fn parse_itl_points(chart_name: &str) -> Option<(u32, u32)> {
    let mut nums = chart_name
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u32>().ok());
    Some((nums.next()?, nums.next()?))
}

pub fn itl_points_for_chart(chart: &deadsync_chart::ChartData, ex_hundredths: u32) -> Option<u32> {
    let (passing_points, max_scoring_points) = parse_itl_points(chart.chart_name.as_str())?;
    Some(itl_points_for_song(
        passing_points,
        max_scoring_points,
        f64::from(ex_hundredths) / 100.0,
    ))
}

pub fn itl_points_for_song(passing_points: u32, max_scoring_points: u32, ex_score: f64) -> u32 {
    let scalar = 40.0_f64;
    let curve = (scalar.powf(ex_score.max(0.0) / scalar) - 1.0)
        * (100.0 / (scalar.powf(100.0 / scalar) - 1.0));
    let percent = ((curve / 100.0) * 1_000_000.0).round() / 1_000_000.0;
    passing_points.saturating_add((f64::from(max_scoring_points) * percent).floor() as u32)
}

fn apply_itl_overall_ranks(
    out: &mut HashMap<String, u32>,
    mut by_chart_points: Vec<(String, u32)>,
) {
    by_chart_points.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut prev_points = None;
    let mut prev_rank = 0u32;
    for (idx, (chart_hash, points)) in by_chart_points.into_iter().enumerate() {
        let rank = if prev_points == Some(points) {
            prev_rank
        } else {
            idx.saturating_add(1) as u32
        };
        out.insert(chart_hash, rank);
        prev_points = Some(points);
        prev_rank = rank;
    }
}

pub fn itl_overall_ranks_from_song_cache(
    song_cache: &[deadsync_chart::SongPack],
    by_chart_score: &HashMap<String, u32>,
) -> HashMap<String, u32> {
    if by_chart_score.is_empty() {
        return HashMap::new();
    }

    let mut single_points = Vec::new();
    let mut double_points = Vec::new();
    for pack in song_cache {
        if !itl_group_name_matches(pack.group_name.as_str()) {
            continue;
        }
        for song in &pack.songs {
            for chart in &song.charts {
                if !chart.has_note_data {
                    continue;
                }
                let Some(ex_hundredths) = by_chart_score.get(chart.short_hash.as_str()).copied()
                else {
                    continue;
                };
                let Some(points) = itl_points_for_chart(chart, ex_hundredths) else {
                    continue;
                };
                if itl_steps_type_from_chart_type(chart.chart_type.as_str())
                    .eq_ignore_ascii_case("double")
                {
                    double_points.push((chart.short_hash.clone(), points));
                } else {
                    single_points.push((chart.short_hash.clone(), points));
                }
            }
        }
    }

    let mut ranks = HashMap::with_capacity(single_points.len() + double_points.len());
    apply_itl_overall_ranks(&mut ranks, single_points);
    apply_itl_overall_ranks(&mut ranks, double_points);
    ranks
}

pub fn itl_judgments_better(cur: &ItlJudgments, prev: &ItlJudgments) -> bool {
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

pub fn itl_clear_type(judgments: &ItlJudgments) -> u8 {
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

#[inline(always)]
pub fn itl_score_from_entry(entry: &ItlHashEntry) -> CachedItlScore {
    CachedItlScore {
        ex_hundredths: entry.ex,
        clear_type: entry.clear_type,
        points: entry.points,
    }
}

#[inline(always)]
fn rank_for_points(sorted_points: &[u32], points: u32) -> Option<u32> {
    sorted_points
        .iter()
        .position(|value| *value == points)
        .map(|idx| idx.saturating_add(1) as u32)
}

pub fn itl_rebuild_song_ranks(data: &mut ItlFileData) {
    let mut points: Vec<u32> = data.hash_map.values().map(|entry| entry.points).collect();
    points.sort_unstable_by(|a, b| b.cmp(a));

    let mut points_single = Vec::with_capacity(points.len());
    let mut points_double = Vec::with_capacity(points.len());
    let mut unknown_points = Vec::new();
    let mut plays_single = 0usize;
    let mut plays_double = 0usize;

    for entry in data.hash_map.values_mut() {
        entry.rank = rank_for_points(points.as_slice(), entry.points);
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
            entry.rank = rank_for_points(points_single.as_slice(), entry.points);
        } else if entry.steps_type.eq_ignore_ascii_case("double") {
            entry.rank = rank_for_points(points_double.as_slice(), entry.points);
        }
    }

    data.points = points;
    data.points_single = points_single;
    data.points_double = points_double;
}

pub fn itl_point_totals(data: &ItlFileData) -> ItlPointTotals {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TMP_ID: AtomicU64 = AtomicU64::new(1);

    fn temp_test_dir(name: &str) -> PathBuf {
        let id = NEXT_TMP_ID.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("deadsync-score-{name}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn sample_chart(chart_type: &str) -> deadsync_chart::ChartData {
        deadsync_chart::ChartData {
            chart_type: chart_type.to_string(),
            difficulty: String::new(),
            description: String::new(),
            chart_name: String::new(),
            meter: 0,
            step_artist: String::new(),
            music_path: None,
            short_hash: String::new(),
            stats: deadsync_chart::ArrowStats::default(),
            tech_counts: deadsync_chart::TechCounts::default(),
            mines_nonfake: 0,
            stamina_counts: deadsync_chart::StaminaCounts::default(),
            total_streams: 0,
            matrix_rating: 0.0,
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
            has_note_data: false,
            has_chart_attacks: false,
            possible_grade_points: 0,
            holds_total: 0,
            rolls_total: 0,
            mines_total: 0,
            display_bpm: None,
            min_bpm: 0.0,
            max_bpm: 0.0,
        }
    }

    fn ranked_chart(hash: &str, chart_type: &str, chart_name: &str) -> deadsync_chart::ChartData {
        let mut chart = sample_chart(chart_type);
        chart.short_hash = hash.to_string();
        chart.chart_name = chart_name.to_string();
        chart.has_note_data = true;
        chart
    }

    fn song_with_charts(charts: Vec<deadsync_chart::ChartData>) -> Arc<deadsync_chart::SongData> {
        Arc::new(deadsync_chart::SongData {
            simfile_path: PathBuf::from("/Songs/ITL Online 2026/Example/song.ssc"),
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
            min_bpm: 0.0,
            max_bpm: 0.0,
            normalized_bpms: String::new(),
            music_length_seconds: 0.0,
            first_second: 0.0,
            total_length_seconds: 0,
            precise_last_second_seconds: 0.0,
            charts,
        })
    }

    fn song_pack(
        group_name: &str,
        charts: Vec<deadsync_chart::ChartData>,
    ) -> deadsync_chart::SongPack {
        deadsync_chart::SongPack {
            group_name: group_name.to_string(),
            name: group_name.to_string(),
            sort_title: String::new(),
            translit_title: String::new(),
            series: String::new(),
            year: 0,
            sync_pref: deadsync_chart::SyncPref::Default,
            directory: PathBuf::new(),
            banner_path: None,
            songs: vec![song_with_charts(charts)],
        }
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
    fn itl_overall_ranks_filter_and_split_chart_points() {
        let song_cache = vec![
            song_pack(
                "ITL Online 2026",
                vec![
                    ranked_chart("single-a", "dance-single", "10 20"),
                    ranked_chart("single-b", "dance-single", "10 20"),
                    ranked_chart("single-c", "dance-single", "10 10"),
                    ranked_chart("double-a", "dance-double", "10 5"),
                    ranked_chart("unscored", "dance-single", "100 100"),
                ],
            ),
            song_pack(
                "Custom Pack",
                vec![ranked_chart("ignored", "dance-single", "500 500")],
            ),
        ];
        let by_chart_score = HashMap::from([
            ("single-a".to_string(), 10_000),
            ("single-b".to_string(), 10_000),
            ("single-c".to_string(), 10_000),
            ("double-a".to_string(), 10_000),
            ("ignored".to_string(), 10_000),
        ]);

        let ranks = itl_overall_ranks_from_song_cache(&song_cache, &by_chart_score);

        assert_eq!(ranks.get("single-a"), Some(&1));
        assert_eq!(ranks.get("single-b"), Some(&1));
        assert_eq!(ranks.get("single-c"), Some(&3));
        assert_eq!(ranks.get("double-a"), Some(&1));
        assert!(!ranks.contains_key("unscored"));
        assert!(!ranks.contains_key("ignored"));
    }

    #[test]
    fn runtime_online_itl_overall_ranks_loads_profile_and_caches_result() {
        let profile_id = "runtime-overall-itl-profile";
        let api_key = "runtime-overall-itl-api";
        let chart_hash = "runtime-overall-itl-chart";
        let song_cache = vec![song_pack(
            "ITL Online 2026",
            vec![ranked_chart(chart_hash, "dance-single", "10 20")],
        )];
        let mut loads = 0;

        let ranks = runtime_online_itl_overall_ranks_for_side(
            0,
            true,
            api_key,
            Some(profile_id),
            314_159,
            &song_cache,
            |loaded_profile| {
                assert_eq!(loaded_profile, profile_id);
                loads += 1;
                HashMap::from([(
                    OnlineItlSelfScoreKey {
                        chart_hash: chart_hash.to_string(),
                        api_key: api_key.to_string(),
                    },
                    10_000,
                )])
            },
        );

        assert_eq!(loads, 1);
        assert_eq!(ranks.get(chart_hash), Some(&1));

        let cached_ranks = runtime_online_itl_overall_ranks_for_side(
            0,
            true,
            api_key,
            Some(profile_id),
            314_159,
            &song_cache,
            |_| {
                loads += 1;
                HashMap::new()
            },
        );

        assert_eq!(loads, 1);
        assert_eq!(cached_ranks.get(chart_hash), Some(&1));
    }

    #[test]
    fn runtime_online_itl_overall_ranks_ignores_unjoined_side() {
        let mut loads = 0;
        let ranks = runtime_online_itl_overall_ranks_for_side(
            0,
            false,
            "runtime-overall-unjoined-api",
            Some("runtime-overall-unjoined-profile"),
            271_828,
            &[],
            |_| {
                loads += 1;
                HashMap::new()
            },
        );

        assert_eq!(loads, 0);
        assert!(ranks.is_empty());
    }

    #[test]
    fn runtime_ensures_itl_score_profile_once() {
        let profile_id = "runtime-ensure-itl-score-profile";
        let chart_hash = "runtime-ensure-itl-score-chart";
        let mut loads = 0;

        runtime_ensure_itl_score_profile_loaded(profile_id, |loaded_profile| {
            assert_eq!(loaded_profile, profile_id);
            loads += 1;
            let mut data = ItlFileData::default();
            data.hash_map.insert(
                chart_hash.to_string(),
                ItlHashEntry {
                    ex: 9876,
                    points: 4321,
                    clear_type: 5,
                    steps_type: "single".to_string(),
                    ..ItlHashEntry::default()
                },
            );
            data
        });
        runtime_ensure_itl_score_profile_loaded(profile_id, |_| {
            loads += 1;
            ItlFileData::default()
        });

        let score = cached_itl_chart_score(profile_id, chart_hash).expect("cached ITL score");
        assert_eq!(loads, 1);
        assert_eq!(score.ex_hundredths, 9876);
        assert_eq!(score.points, 4321);
        assert_eq!(score.clear_type, 5);
    }

    #[test]
    fn runtime_import_itl_json_writes_valid_payload() {
        let profile_id = "runtime-import-itl-profile";
        let json_text = serde_json::to_string(&json!({
            "pathMap": { "/Songs/ITL Online 2026/Example": "runtime-import-itl-chart" },
            "hashMap": {
                "runtime-import-itl-chart": { "ex": 98.76, "points": 4321 }
            }
        }))
        .unwrap();
        let mut saved_profile = String::new();
        let mut saved_hashes = 0usize;

        let count = runtime_import_itl_json(profile_id, &json_text, |profile, data| {
            saved_profile = profile.to_string();
            saved_hashes = data.hash_map.len();
        });

        assert_eq!(count, 1);
        assert_eq!(saved_profile, profile_id);
        assert_eq!(saved_hashes, 1);
    }

    #[test]
    fn runtime_update_itl_unlock_folders_writes_changed_profile() {
        let profile_id = "runtime-update-itl-unlocks-profile";
        let mut reads = 0;
        let mut saved = None;

        let changed = runtime_update_itl_unlock_folders(
            profile_id,
            ["/Songs/Unlock/A", "/Songs/Unlock/B"],
            |loaded_profile| {
                assert_eq!(loaded_profile, profile_id);
                reads += 1;
                ItlFileData::default()
            },
            |saved_profile, data| {
                assert_eq!(saved_profile, profile_id);
                saved = Some(data.unlock_folders.len());
            },
        );

        assert!(changed);
        assert_eq!(reads, 1);
        assert_eq!(saved, Some(2));
        assert!(cached_itl_song_folder_unlocked(
            profile_id,
            "/Songs/Unlock/A"
        ));
        assert!(cached_itl_song_folder_unlocked(
            profile_id,
            "/Songs/Unlock/B"
        ));
    }

    #[test]
    fn runtime_update_itl_unlock_folders_skips_unchanged_profile() {
        let profile_id = "runtime-update-itl-unlocks-unchanged-profile";
        let mut writes = 0;

        let changed = runtime_update_itl_unlock_folders(
            profile_id,
            ["/Songs/Unlock/A"],
            |_| {
                let mut data = ItlFileData::default();
                data.unlock_folders
                    .insert("/Songs/Unlock/A".to_string(), true);
                data
            },
            |_, _| {
                writes += 1;
            },
        );

        assert!(!changed);
        assert_eq!(writes, 0);
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
    fn itl_judgments_from_counts_maps_all_fields() {
        let judgments = itl_judgments_from_counts(ItlJudgmentCountsInput {
            fantastic_plus: 1,
            fantastic: 2,
            excellent: 3,
            great: 4,
            decent: 5,
            way_off: 6,
            miss: 7,
            total_steps: 8,
            holds_held: 9,
            total_holds: 10,
            mines_hit: 11,
            total_mines: 12,
            rolls_held: 13,
            total_rolls: 14,
        });

        assert_eq!(judgments.w0, 1);
        assert_eq!(judgments.w1, 2);
        assert_eq!(judgments.w2, 3);
        assert_eq!(judgments.w3, 4);
        assert_eq!(judgments.w4, 5);
        assert_eq!(judgments.w5, 6);
        assert_eq!(judgments.miss, 7);
        assert_eq!(judgments.total_steps, 8);
        assert_eq!(judgments.holds, 9);
        assert_eq!(judgments.total_holds, 10);
        assert_eq!(judgments.mines, 11);
        assert_eq!(judgments.total_mines, 12);
        assert_eq!(judgments.rolls, 13);
        assert_eq!(judgments.total_rolls, 14);
    }

    #[test]
    fn itl_timing_and_current_score_helpers_normalize_inputs() {
        assert!(itl_timing_windows_all_enabled(&[]));
        assert!(itl_timing_windows_all_enabled(&[false, false]));
        assert!(!itl_timing_windows_all_enabled(&[false, true]));

        let notes: Vec<deadsync_rules::note::Note> = Vec::new();
        let note_times: Vec<deadsync_core::song_time::SongTimeNs> = Vec::new();
        let hold_end_times: Vec<Option<deadsync_core::song_time::SongTimeNs>> = Vec::new();
        assert_eq!(
            itl_current_score_hundredths(ItlScoreCalcInput {
                notes: notes.as_slice(),
                note_times: note_times.as_slice(),
                hold_end_times: hold_end_times.as_slice(),
                total_steps: 0,
                holds_total: 0,
                rolls_total: 0,
                mines_total: 0,
                fail_time: None,
            }),
            0
        );
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
    fn itl_data_from_json_parses_and_guards() {
        let text = serde_json::to_string(&json!({
            "pathMap": { "/Songs/ITL Online 2026/Example": "deadbeefcafebabe" },
            "hashMap": {
                "deadbeefcafebabe": { "ex": 94.37, "points": 4200, "clearType": 5 }
            },
            "unlockFolders": { "/Songs/ITL Online 2026/Example": true }
        }))
        .unwrap();
        let data = itl_data_from_json(&text).expect("parses");
        assert_eq!(data.hash_map.len(), 1);
        assert_eq!(data.hash_map["deadbeefcafebabe"].ex, 9437);
        assert_eq!(data.path_map.len(), 1);
        assert!(data.unlock_folders["/Songs/ITL Online 2026/Example"]);

        assert!(itl_data_from_json("{}").is_none());
        assert!(itl_data_from_json("not json").is_none());
        assert!(itl_data_from_json(r#"{"hashMap":{}}"#).is_none());
    }

    #[test]
    fn online_itl_self_score_index_round_trips() {
        let dir = temp_test_dir("itl-self-score");
        let path = dir.join("itl_self.bin");
        let key = OnlineItlSelfScoreKey {
            chart_hash: "deadbeefcafebabe".to_string(),
            api_key: "api-key".to_string(),
        };
        let mut expected = HashMap::new();
        expected.insert(key, 9912);

        let in_memory: OnlineItlSelfCacheMap = expected.clone().into_iter().collect();
        save_online_itl_self_index_file(&path, &in_memory).expect("save index");

        assert_eq!(load_online_itl_self_index_file(&path), Some(expected));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn online_itl_self_index_paths_use_profile_score_layout() {
        let profile_dir = PathBuf::from("Profile");

        assert_eq!(
            online_itl_self_index_path(&profile_dir, OnlineItlSelfIndexKind::Score),
            profile_dir.join("scores").join("gs").join("itl_self.bin")
        );
        assert_eq!(
            online_itl_self_index_path(&profile_dir, OnlineItlSelfIndexKind::Rank),
            profile_dir.join("scores").join("gs").join("itl_rank.bin")
        );
    }

    #[test]
    fn online_itl_self_index_profile_dir_helpers_round_trip() {
        let dir = temp_test_dir("itl-self-profile-dir");
        let key = OnlineItlSelfScoreKey {
            chart_hash: "profile-dir-chart".to_string(),
            api_key: "profile-dir-api".to_string(),
        };
        let in_memory: OnlineItlSelfCacheMap = [(key.clone(), 1234)].into_iter().collect();

        let saved_path = save_online_itl_self_index_for_profile_dir(
            &dir,
            OnlineItlSelfIndexKind::Rank,
            &in_memory,
        )
        .expect("save index");

        assert_eq!(
            saved_path,
            dir.join("scores").join("gs").join("itl_rank.bin")
        );
        assert_eq!(
            load_online_itl_self_index_for_profile_dir(&dir, OnlineItlSelfIndexKind::Rank)
                .get(&key),
            Some(&1234)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn itl_profile_file_helpers_use_profile_root() {
        let profile_dir = PathBuf::from("Profile");

        assert_eq!(
            itl_profile_file_path(&profile_dir),
            profile_dir.join("ITL2026.json")
        );
    }

    #[test]
    fn itl_profile_file_helpers_round_trip_and_default_missing() {
        let dir = temp_test_dir("itl-profile-file");
        let mut data = ItlFileData::default();
        data.hash_map.insert(
            "profile-file-chart".to_string(),
            ItlHashEntry {
                ex: 9900,
                points: 100,
                clear_type: 5,
                no_cmod: true,
                rank: Some(1),
                ..ItlHashEntry::default()
            },
        );

        let saved_path = write_itl_file_for_profile_dir(&dir, &data).expect("write itl file");

        assert_eq!(saved_path, dir.join("ITL2026.json"));
        assert_eq!(
            read_itl_file_or_default_for_profile_dir(&dir)
                .expect("read itl file")
                .hash_map["profile-file-chart"]
                .ex,
            9900
        );

        let missing_dir = temp_test_dir("itl-profile-file-missing");
        assert!(
            read_itl_file_or_default_for_profile_dir(&missing_dir)
                .expect("missing file defaults")
                .is_empty()
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&missing_dir);
    }

    #[test]
    fn runtime_ensures_online_itl_self_score_profile_once() {
        let profile_id = "runtime-ensure-itl-self-score-profile";
        let api_key = "runtime-ensure-itl-self-score-api";
        let chart_hash = "runtime-ensure-itl-self-score-chart";
        let mut loads = 0;

        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, |loaded_profile| {
            assert_eq!(loaded_profile, profile_id);
            loads += 1;
            HashMap::from([(
                OnlineItlSelfScoreKey {
                    chart_hash: chart_hash.to_string(),
                    api_key: api_key.to_string(),
                },
                9876,
            )])
        });
        runtime_ensure_online_itl_self_score_profile_loaded(profile_id, |_| {
            loads += 1;
            HashMap::new()
        });

        assert_eq!(loads, 1);
        assert_eq!(
            get_online_itl_self_score(chart_hash, Some(profile_id), api_key),
            Some(9876)
        );
    }

    #[test]
    fn runtime_sets_online_itl_self_score_and_saves_snapshot() {
        let profile_id = "runtime-set-itl-self-score-profile";
        let api_key = "runtime-set-itl-self-score-api";
        let chart_hash = "runtime-set-itl-self-score-chart";
        let mut loads = 0;
        let mut saved = None;

        runtime_set_online_itl_self_score(
            Some(profile_id),
            api_key,
            chart_hash,
            Some(9988),
            |loaded_profile| {
                assert_eq!(loaded_profile, profile_id);
                loads += 1;
                HashMap::new()
            },
            |saved_profile, by_key| {
                assert_eq!(saved_profile, profile_id);
                saved = by_key
                    .get(&OnlineItlSelfScoreKey {
                        chart_hash: chart_hash.to_string(),
                        api_key: api_key.to_string(),
                    })
                    .copied();
            },
        );

        assert_eq!(loads, 1);
        assert_eq!(saved, Some(9988));
        assert_eq!(
            get_online_itl_self_score(chart_hash, Some(profile_id), api_key),
            Some(9988)
        );
    }

    #[test]
    fn runtime_ensures_online_itl_self_rank_profile_once() {
        let profile_id = "runtime-ensure-itl-self-rank-profile";
        let api_key = "runtime-ensure-itl-self-rank-api";
        let chart_hash = "runtime-ensure-itl-self-rank-chart";
        let mut loads = 0;

        runtime_ensure_online_itl_self_rank_profile_loaded(profile_id, |loaded_profile| {
            assert_eq!(loaded_profile, profile_id);
            loads += 1;
            HashMap::from([(
                OnlineItlSelfScoreKey {
                    chart_hash: chart_hash.to_string(),
                    api_key: api_key.to_string(),
                },
                42,
            )])
        });
        runtime_ensure_online_itl_self_rank_profile_loaded(profile_id, |_| {
            loads += 1;
            HashMap::new()
        });

        assert_eq!(loads, 1);
        assert_eq!(
            get_online_itl_self_rank(chart_hash, Some(profile_id), api_key),
            Some(42)
        );
    }

    #[test]
    fn runtime_sets_online_itl_self_rank_and_saves_snapshot() {
        let profile_id = "runtime-set-itl-self-rank-profile";
        let api_key = "runtime-set-itl-self-rank-api";
        let chart_hash = "runtime-set-itl-self-rank-chart";
        let mut loads = 0;
        let mut saved = None;

        runtime_set_online_itl_self_rank(
            Some(profile_id),
            api_key,
            chart_hash,
            Some(7),
            |loaded_profile| {
                assert_eq!(loaded_profile, profile_id);
                loads += 1;
                HashMap::new()
            },
            |saved_profile, by_key| {
                assert_eq!(saved_profile, profile_id);
                saved = by_key
                    .get(&OnlineItlSelfScoreKey {
                        chart_hash: chart_hash.to_string(),
                        api_key: api_key.to_string(),
                    })
                    .copied();
            },
        );

        assert_eq!(loads, 1);
        assert_eq!(saved, Some(7));
        assert_eq!(
            get_online_itl_self_rank(chart_hash, Some(profile_id), api_key),
            Some(7)
        );
    }

    #[test]
    fn runtime_cached_itl_chart_score_loads_profile_once() {
        let profile_id = "runtime-cached-itl-chart-profile";
        let chart_hash = "runtime-cached-itl-chart";
        let mut loads = 0;

        let score = runtime_cached_itl_chart_score(Some(profile_id), chart_hash, |loaded| {
            assert_eq!(loaded, profile_id);
            loads += 1;
            let mut data = ItlFileData::default();
            data.hash_map.insert(
                chart_hash.to_string(),
                ItlHashEntry {
                    ex: 9876,
                    clear_type: 5,
                    points: 1234,
                    ..ItlHashEntry::default()
                },
            );
            data
        })
        .expect("cached score");

        assert_eq!(score.ex_hundredths, 9876);
        assert_eq!(score.clear_type, 5);
        assert_eq!(score.points, 1234);

        let score = runtime_cached_itl_chart_score(Some(profile_id), chart_hash, |_| {
            loads += 1;
            ItlFileData::default()
        })
        .expect("cached score remains loaded");
        assert_eq!(score.ex_hundredths, 9876);
        assert_eq!(loads, 1);
    }

    #[test]
    fn runtime_cached_itl_song_folder_unlocked_loads_profile() {
        let profile_id = "runtime-cached-itl-folder-profile";
        let folder = "/Songs/ITL Online 2026/Example";

        assert!(runtime_cached_itl_song_folder_unlocked(
            folder,
            Some(profile_id),
            |loaded| {
                assert_eq!(loaded, profile_id);
                let mut data = ItlFileData::default();
                data.unlock_folders.insert(folder.to_string(), true);
                data
            }
        ));
        assert!(!runtime_cached_itl_song_folder_unlocked(
            "/Songs/Other",
            Some(profile_id),
            |_| ItlFileData::default()
        ));
    }

    #[test]
    fn runtime_cached_online_itl_self_score_loads_and_trims() {
        let profile_id = "runtime-cached-online-itl-score-profile";
        let api_key = "runtime-cached-online-itl-score-api";
        let chart_hash = "runtime-cached-online-itl-score-chart";
        let mut loads = 0;

        let score = runtime_cached_online_itl_self_score(
            chart_hash,
            Some(" runtime-cached-online-itl-score-profile "),
            api_key,
            |loaded| {
                assert_eq!(loaded, profile_id);
                loads += 1;
                HashMap::from([(
                    OnlineItlSelfScoreKey {
                        chart_hash: chart_hash.to_string(),
                        api_key: api_key.to_string(),
                    },
                    9999,
                )])
            },
        );

        assert_eq!(score, Some(9999));
        assert_eq!(
            runtime_cached_online_itl_self_score_assume_loaded(
                chart_hash,
                Some(profile_id),
                api_key
            ),
            Some(9999)
        );
        assert_eq!(loads, 1);
    }

    #[test]
    fn runtime_cached_online_itl_self_rank_loads_and_trims() {
        let profile_id = "runtime-cached-online-itl-rank-profile";
        let api_key = "runtime-cached-online-itl-rank-api";
        let chart_hash = "runtime-cached-online-itl-rank-chart";

        let rank = runtime_cached_online_itl_self_rank(
            chart_hash,
            Some(" runtime-cached-online-itl-rank-profile "),
            api_key,
            |loaded| {
                assert_eq!(loaded, profile_id);
                HashMap::from([(
                    OnlineItlSelfScoreKey {
                        chart_hash: chart_hash.to_string(),
                        api_key: api_key.to_string(),
                    },
                    17,
                )])
            },
        );

        assert_eq!(rank, Some(17));
        assert_eq!(
            runtime_cached_online_itl_self_rank_assume_loaded(
                chart_hash,
                Some(profile_id),
                api_key
            ),
            Some(17)
        );
    }

    #[test]
    fn itl_classification_helpers_match_event_rules() {
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/Custom Pack/Example".to_string(),
            "deadbeefcafebabe".to_string(),
        );

        assert!(itl_group_name_matches("ITL Online 2026"));
        assert!(itl_group_name_matches("Some ITL 2026 Folder"));
        assert!(!itl_group_name_matches("Custom Pack"));
        assert!(itl_song_matches(
            Some("/Songs/Custom Pack/Example"),
            None,
            &data
        ));
        assert!(itl_song_matches(None, Some("ITL Online 2026"), &data));
        assert!(!itl_song_matches(None, Some("Custom Pack"), &data));
        assert!(itl_chart_no_cmod("(NO CMOD)", None));
        assert!(!itl_chart_no_cmod(
            "No marker",
            Some(&ItlHashEntry {
                no_cmod: false,
                ..ItlHashEntry::default()
            })
        ));
        assert_eq!(
            itl_event_name_from_group(Some("ITL Online 2026")),
            "ITL Online 2026"
        );
        assert_eq!(itl_event_name_from_group(None), "ITL Online 2026");
        assert_eq!(itl_steps_type_from_chart_type("dance-double"), "double");
        assert_eq!(itl_steps_type_from_chart_type("dance-single"), "single");
    }

    #[test]
    fn itl_unlock_pack_names_match_legacy_pattern() {
        assert!(is_itl_unlocks_pack("ITL Online 2023 Unlocks"));
        assert!(is_itl_unlocks_pack("itl online 2024 unlocks"));
        assert!(is_itl_unlocks_pack("ITL ONLINE 2022 UNLOCKS"));
        assert!(is_itl_unlocks_pack("  ITL Online 2025 Unlocks  "));

        assert!(!is_itl_unlocks_pack("ITL Online 23 Unlocks"));
        assert!(!is_itl_unlocks_pack("ITL Online 20XX Unlocks"));
        assert!(!is_itl_unlocks_pack("ITL Online 2023 Locks"));
        assert!(!is_itl_unlocks_pack("ITL Offline 2023 Unlocks"));
        assert!(!is_itl_unlocks_pack("ITL Online 2023  Unlocks"));
        assert!(!is_itl_unlocks_pack("ITL Online 2023 Unlocks Extra"));
        assert!(!is_itl_unlocks_pack(""));
    }

    #[test]
    fn itl_unlock_folder_helpers_trim_and_report_changes() {
        let mut data = ItlFileData::default();
        assert!(data.is_empty());

        assert!(itl_mark_unlock_folders(
            &mut data,
            [" /Songs/Unlock/A ", "", " /Songs/Unlock/B "]
        ));
        assert!(!data.is_empty());
        assert!(itl_song_folder_unlocked(&data, "/Songs/Unlock/A"));
        assert!(itl_song_folder_unlocked(&data, "/Songs/Unlock/B"));
        assert!(!itl_song_folder_unlocked(&data, "/Songs/Unlock/C"));
        assert!(!itl_mark_unlock_folders(&mut data, ["/Songs/Unlock/A"]));
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
}
