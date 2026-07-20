use bincode::{Decode, Encode};
use chrono::{DateTime, TimeZone, Utc};
use deadsync_core::input::InputSource;
use deadsync_core::note::NoteType;
use deadsync_core::song_time::{SongTimeNs, song_time_ns_from_seconds, song_time_ns_invalid};
use deadsync_rules::note::{HoldResult, MineResult, Note};
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::{judgment, timing};
use log::{debug, warn};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub mod column_judgments;
pub mod event_progress;
pub mod import;
pub mod itl;
pub mod leaderboard;
pub mod local_store;
pub mod select_music;
pub mod stage_stats;
pub use column_judgments::*;
pub use event_progress::*;
pub use import::{ImportedHighScore, grade_from_itg, local_score_from_itg, parse_itg_datetime_ms};
pub use itl::{
    ItlFileData, ItlFileReadError, ItlFileWriteError, ItlHashEntry, ItlJudgmentCountsInput,
    ItlJudgments, ItlPointTotals, ItlScoreCacheState, ItlScoreCalcInput,
    OnlineItlOverallRankCacheKey, OnlineItlSelfCacheMap, OnlineItlSelfCacheState,
    OnlineItlSelfIndexKind, OnlineItlSelfIndexMap, OnlineItlSelfIndexWriteError,
    OnlineItlSelfScoreKey, cached_itl_chart_no_cmod_for_song, cached_itl_chart_score,
    cached_itl_song_folder_unlocked, cached_itl_song_score,
    cached_online_itl_overall_ranks_for_side, empty_online_itl_overall_ranks, ex_hundredths,
    get_online_itl_self_rank, get_online_itl_self_score, get_online_srpg_self_score,
    insert_itl_score_profile, insert_online_itl_self_rank_profile,
    insert_online_itl_self_score_profile, insert_online_srpg_self_score_profile,
    is_itl_unlocks_pack, itl_chart_no_cmod, itl_clear_type, itl_current_score_hundredths,
    itl_data_from_json, itl_event_name_from_group, itl_ex_score_percent, itl_group_name_matches,
    itl_judgments_better, itl_judgments_from_counts, itl_judgments_from_groovestats_counts,
    itl_mark_unlock_folders, itl_overall_ranks_from_song_cache, itl_point_totals,
    itl_points_for_chart, itl_points_for_song, itl_profile_file_path, itl_rebuild_song_ranks,
    itl_score_for_song, itl_score_from_entry, itl_score_profile_loaded, itl_song_dir,
    itl_song_folder_unlocked, itl_song_matches, itl_song_matches_context,
    itl_steps_type_from_chart_type, itl_timing_windows_all_enabled,
    load_online_itl_self_index_file, load_online_itl_self_index_for_profile_dir,
    mark_itl_unlock_folders, online_itl_self_index_path, online_itl_self_rank_profile_loaded,
    online_itl_self_score_generation, online_itl_self_score_profile_loaded,
    online_itl_self_scores_by_chart_for_api, online_srpg_self_score_profile_loaded,
    parse_itl_points, read_itl_file_from_path, read_itl_file_or_default_for_profile_dir,
    runtime_cached_itl_chart_score, runtime_cached_itl_song_folder_unlocked,
    runtime_cached_itl_song_score, runtime_cached_itl_song_score_assume_loaded,
    runtime_cached_online_itl_self_rank, runtime_cached_online_itl_self_rank_assume_loaded,
    runtime_cached_online_itl_self_rank_for_profile_dirs, runtime_cached_online_itl_self_score,
    runtime_cached_online_itl_self_score_assume_loaded,
    runtime_cached_online_itl_self_score_for_profile_dirs, runtime_cached_online_srpg_self_score,
    runtime_cached_online_srpg_self_score_assume_loaded,
    runtime_cached_online_srpg_self_score_for_profile_dirs,
    runtime_ensure_itl_score_profile_loaded, runtime_ensure_itl_wheel_caches_loaded,
    runtime_ensure_itl_wheel_caches_loaded_for_profile_dirs,
    runtime_ensure_online_itl_self_rank_profile_loaded,
    runtime_ensure_online_itl_self_score_profile_loaded,
    runtime_ensure_online_srpg_self_score_profile_loaded, runtime_import_itl_json,
    runtime_load_online_itl_self_index_for_profile, runtime_online_itl_overall_ranks_for_side,
    runtime_read_itl_file_for_profile, runtime_save_online_itl_self_index_for_profile,
    runtime_set_itl_score_file, runtime_set_online_itl_self_rank,
    runtime_set_online_itl_self_rank_for_profile_dirs, runtime_set_online_itl_self_score,
    runtime_set_online_itl_self_score_for_profile_dirs, runtime_set_online_srpg_self_score,
    runtime_set_online_srpg_self_score_for_profile_dirs, runtime_update_itl_unlock_folders,
    runtime_write_itl_file_for_profile, save_online_itl_self_index_file,
    save_online_itl_self_index_for_profile_dir, set_itl_score_profile, set_online_itl_self_rank,
    set_online_itl_self_score, set_online_srpg_self_score, store_online_itl_overall_ranks_for_side,
    write_itl_file_for_profile_dir, write_itl_file_to_path,
};
pub use leaderboard::*;
pub use local_store::*;
pub use select_music::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
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
    #[inline(always)]
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

    /// Short display string used in SMX pad GIF filenames (`results_25@<suffix>.gif`).
    pub const fn gif_suffix(&self) -> &'static str {
        match self {
            Self::Quint => "star5",
            Self::Tier01 => "star4",
            Self::Tier02 => "star3",
            Self::Tier03 => "star2",
            Self::Tier04 => "star1",
            Self::Tier05 => "S+",
            Self::Tier06 => "S",
            Self::Tier07 => "S-",
            Self::Tier08 => "A+",
            Self::Tier09 => "A",
            Self::Tier10 => "A-",
            Self::Tier11 => "B+",
            Self::Tier12 => "B",
            Self::Tier13 => "B-",
            Self::Tier14 => "C+",
            Self::Tier15 => "C",
            Self::Tier16 => "C-",
            Self::Tier17 => "D",
            Self::Failed => "F",
        }
    }

    /// The "base" grade to fall back to for `+`/`-` variants when no exact gif exists.
    /// `S+` and `S-` fall back to `S`, `A+`/`A-` to `A`, etc. Base grades return `None`.
    pub const fn gif_base(&self) -> Option<Grade> {
        match self {
            Self::Tier05 | Self::Tier07 => Some(Self::Tier06),
            Self::Tier08 | Self::Tier10 => Some(Self::Tier09),
            Self::Tier11 | Self::Tier13 => Some(Self::Tier12),
            Self::Tier14 | Self::Tier16 => Some(Self::Tier15),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct CachedScore {
    pub grade: Grade,
    pub score_percent: f64,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalScalarScore {
    pub percent: f64,
    pub is_fail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Encode, Decode)]
pub struct LocalScoreBestScalar {
    pub grade: Grade,
    pub percent: f64,
}

#[derive(Debug, Default, Clone, PartialEq, Encode, Decode)]
pub struct LocalScoreIndex {
    pub best_itg: HashMap<String, CachedScore>,
    pub best_ex: HashMap<String, LocalScoreBestScalar>,
    pub best_hard_ex: HashMap<String, LocalScoreBestScalar>,
    pub best_pass_rate: HashMap<String, u32>,
}

#[derive(Default)]
pub struct GsScoreCacheState {
    pub loaded_profiles: HashMap<String, HashMap<String, CachedScore>>,
}

#[derive(Default)]
pub struct AcScoreCacheState {
    pub loaded_profiles: HashMap<String, HashMap<String, ArrowCloudScores>>,
}

#[derive(Default)]
pub struct LocalScoreCacheState {
    pub loaded_profiles: HashMap<String, LocalScoreIndex>,
}

#[derive(Clone, Debug)]
pub struct MachineBest {
    pub score: CachedScore,
    pub initials: String,
}

#[derive(Default)]
pub struct MachineLocalScoreCacheState {
    pub loaded: bool,
    pub best_itg: HashMap<String, MachineBest>,
}

impl GsScoreCacheState {
    #[inline(always)]
    pub fn profile_is_loaded(&self, profile_id: &str) -> bool {
        self.loaded_profiles.contains_key(profile_id)
    }

    pub fn insert_loaded_profile(
        &mut self,
        profile_id: &str,
        by_chart: HashMap<String, CachedScore>,
    ) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_insert(by_chart);
    }

    pub fn get_profile_score(&self, profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|scores| scores.get(chart_hash).copied())
    }

    pub fn profile_chart_hashes(&self, profile_id: &str) -> HashSet<String> {
        self.loaded_profiles
            .get(profile_id)
            .map_or_else(HashSet::new, |scores| scores.keys().cloned().collect())
    }

    pub fn set_profile_score(
        &mut self,
        profile_id: &str,
        chart_hash: String,
        score: CachedScore,
    ) -> Option<HashMap<String, CachedScore>> {
        let map = self.loaded_profiles.get_mut(profile_id)?;
        map.insert(chart_hash, fix_gs_cached_score(score));
        Some(map.clone())
    }

    pub fn seed_profile_score(&mut self, profile_id: &str, chart_hash: &str, score: CachedScore) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_default()
            .insert(chart_hash.to_string(), score);
    }
}

impl AcScoreCacheState {
    #[inline(always)]
    pub fn profile_is_loaded(&self, profile_id: &str) -> bool {
        self.loaded_profiles.contains_key(profile_id)
    }

    pub fn insert_loaded_profile(
        &mut self,
        profile_id: &str,
        by_chart: HashMap<String, ArrowCloudScores>,
    ) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_insert(by_chart);
    }

    pub fn get_profile_scores(
        &self,
        profile_id: &str,
        chart_hash: &str,
    ) -> Option<ArrowCloudScores> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|scores| scores.get(chart_hash).copied())
    }

    pub fn profile_chart_hashes_with_itg(&self, profile_id: &str) -> HashSet<String> {
        self.loaded_profiles
            .get(profile_id)
            .map_or_else(HashSet::new, |scores| {
                scores
                    .iter()
                    .filter_map(|(hash, ac)| ac.itg.is_some().then(|| hash.clone()))
                    .collect()
            })
    }

    pub fn set_profile_scores_bulk(
        &mut self,
        profile_id: &str,
        entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
    ) -> Option<HashMap<String, ArrowCloudScores>> {
        let map = self.loaded_profiles.get_mut(profile_id)?;
        for (hash, scores) in entries {
            map.insert(hash, scores);
        }
        Some(map.clone())
    }

    pub fn merge_profile_submit_scores(
        &mut self,
        profile_id: &str,
        chart_hash: &str,
        itg_percent: f64,
        ex_percent: f64,
        hard_ex_percent: f64,
        is_fail: bool,
        submitted_at: DateTime<Utc>,
    ) -> Option<HashMap<String, ArrowCloudScores>> {
        let new_scores = ArrowCloudScores {
            itg: arrowcloud_score_from_submit_percent(itg_percent, is_fail, submitted_at),
            ex: arrowcloud_score_from_submit_percent(ex_percent, is_fail, submitted_at),
            hard_ex: arrowcloud_score_from_submit_percent(hard_ex_percent, is_fail, submitted_at),
        };

        let map = self.loaded_profiles.get_mut(profile_id)?;
        let entry = map.entry(chart_hash.to_string()).or_default();
        merge_arrowcloud_score_slot(&mut entry.itg, new_scores.itg);
        merge_arrowcloud_score_slot(&mut entry.ex, new_scores.ex);
        merge_arrowcloud_score_slot(&mut entry.hard_ex, new_scores.hard_ex);
        Some(map.clone())
    }
}

impl LocalScoreCacheState {
    #[inline(always)]
    pub fn profile_is_loaded(&self, profile_id: &str) -> bool {
        self.loaded_profiles.contains_key(profile_id)
    }

    pub fn insert_loaded_profile(&mut self, profile_id: &str, index: LocalScoreIndex) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_insert(index);
    }

    pub fn get_profile_itg_score(&self, profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|idx| idx.best_itg.get(chart_hash).copied())
    }

    pub fn get_profile_pass_rate(&self, profile_id: &str, chart_hash: &str) -> Option<u32> {
        self.loaded_profiles
            .get(profile_id)
            .and_then(|idx| idx.best_pass_rate.get(chart_hash).copied())
    }

    pub fn get_profile_scalar_score(
        &self,
        profile_id: &str,
        chart_hash: &str,
        hard_ex: bool,
    ) -> Option<LocalScalarScore> {
        let index = self.loaded_profiles.get(profile_id)?;
        let best = if hard_ex {
            index.best_hard_ex.get(chart_hash)
        } else {
            index.best_ex.get(chart_hash)
        }?;
        Some(LocalScalarScore {
            percent: best.percent,
            is_fail: best.grade == Grade::Failed,
        })
    }

    pub fn update_loaded_profile_index(
        &mut self,
        profile_id: &str,
        chart_hash: &str,
        header: &LocalScoreHeader,
    ) -> Option<LocalScoreIndex> {
        let index = self.loaded_profiles.get_mut(profile_id)?;
        update_local_score_index(index, chart_hash, header);
        Some(index.clone())
    }

    pub fn seed_profile_itg_score(
        &mut self,
        profile_id: &str,
        chart_hash: &str,
        score: CachedScore,
    ) {
        self.loaded_profiles
            .entry(profile_id.to_string())
            .or_default()
            .best_itg
            .insert(chart_hash.to_string(), score);
    }
}

impl MachineLocalScoreCacheState {
    pub fn record(&self, chart_hash: &str) -> Option<(String, CachedScore)> {
        self.best_itg
            .get(chart_hash)
            .map(|score| (score.initials.clone(), score.score))
    }

    pub fn update_if_loaded(&mut self, chart_hash: &str, score: CachedScore, initials: &str) {
        if !self.loaded {
            return;
        }
        match self.best_itg.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_itg(&score, &existing.score) {
                    existing.score = score;
                    existing.initials = initials.to_string();
                }
            }
            None => {
                self.best_itg.insert(
                    chart_hash.to_string(),
                    MachineBest {
                        score,
                        initials: initials.to_string(),
                    },
                );
            }
        }
    }
}

static RUNTIME_GS_SCORE_CACHE: LazyLock<Mutex<GsScoreCacheState>> =
    LazyLock::new(|| Mutex::new(GsScoreCacheState::default()));
static RUNTIME_AC_SCORE_CACHE: LazyLock<Mutex<AcScoreCacheState>> =
    LazyLock::new(|| Mutex::new(AcScoreCacheState::default()));
static RUNTIME_LOCAL_SCORE_CACHE: LazyLock<Mutex<LocalScoreCacheState>> =
    LazyLock::new(|| Mutex::new(LocalScoreCacheState::default()));
static RUNTIME_MACHINE_LOCAL_SCORE_CACHE: LazyLock<Mutex<MachineLocalScoreCacheState>> =
    LazyLock::new(|| Mutex::new(MachineLocalScoreCacheState::default()));

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreCacheLoadKind {
    GrooveStats,
    Local,
    MachineLocal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreCacheLoadReport {
    pub kind: ScoreCacheLoadKind,
    pub profile_id: Option<String>,
    pub primary_entries: usize,
    pub secondary_entries: usize,
    pub elapsed_ms: f64,
}

#[derive(Debug, Default)]
pub struct ScoreCacheRuntimeResult {
    pub load_report: Option<ScoreCacheLoadReport>,
    pub write_errors: Vec<ScoreIndexWriteError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreCacheRuntimeKind {
    GrooveStats,
    ArrowCloud,
    Local,
    MachineLocal,
}

pub fn log_score_index_write_error(kind: &str, error: ScoreIndexWriteError) {
    match error {
        ScoreIndexWriteError::CreateDir { dir, error } => {
            warn!("Failed to create {kind} score index dir {dir:?}: {error}");
        }
        ScoreIndexWriteError::Encode { path } => {
            warn!("Failed to encode {kind} score index at {path:?}");
        }
        ScoreIndexWriteError::WriteTemp { tmp_path, error } => {
            warn!("Failed to write {kind} score index temp file {tmp_path:?}: {error}");
        }
        ScoreIndexWriteError::Commit {
            path,
            tmp_path: _,
            error,
        } => {
            warn!("Failed to commit {kind} score index file {path:?}: {error}");
        }
    }
}

fn log_score_cache_load(report: ScoreCacheLoadReport) {
    if report.elapsed_ms < 25.0 {
        return;
    }
    match report.kind {
        ScoreCacheLoadKind::GrooveStats => {
            let profile_id = report.profile_id.unwrap_or_default();
            let loaded_entries = report.primary_entries;
            let load_ms = report.elapsed_ms;
            debug!(
                "Loaded GrooveStats score cache for profile {profile_id}: {loaded_entries} chart(s) in {load_ms:.2}ms."
            );
        }
        ScoreCacheLoadKind::Local => {
            let profile_id = report.profile_id.unwrap_or_default();
            let loaded_itg = report.primary_entries;
            let loaded_ex = report.secondary_entries;
            let load_ms = report.elapsed_ms;
            debug!(
                "Loaded local score cache for profile {profile_id}: ITG={loaded_itg}, EX={loaded_ex} in {load_ms:.2}ms."
            );
        }
        ScoreCacheLoadKind::MachineLocal => {
            let total = report.primary_entries;
            let load_ms = report.elapsed_ms;
            debug!("Loaded machine local score cache: {total} chart(s) in {load_ms:.2}ms.");
        }
    }
}

pub fn log_score_cache_result(kind: &str, result: ScoreCacheRuntimeResult) {
    for error in result.write_errors {
        log_score_index_write_error(kind, error);
    }
    if let Some(report) = result.load_report {
        log_score_cache_load(report);
    }
}

pub fn score_cache_kind_label(kind: ScoreCacheRuntimeKind) -> &'static str {
    match kind {
        ScoreCacheRuntimeKind::GrooveStats => "GS",
        ScoreCacheRuntimeKind::ArrowCloud => "AC",
        ScoreCacheRuntimeKind::Local | ScoreCacheRuntimeKind::MachineLocal => "local",
    }
}

pub fn log_score_cache_results(
    results: impl IntoIterator<Item = (ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)>,
) {
    for (kind, result) in results {
        log_score_cache_result(score_cache_kind_label(kind), result);
    }
}

pub fn log_score_store_write_error(kind: &str, chart_hash: &str, error: ScoreStoreWriteError) {
    match error {
        ScoreStoreWriteError::CreateDir { dir, error } => {
            warn!("Failed to create {kind} score dir {dir:?}: {error}");
        }
        ScoreStoreWriteError::Encode { .. } => {
            warn!("Failed to encode {kind} score for chart {chart_hash}");
        }
        ScoreStoreWriteError::WriteFile { path, error } => {
            warn!("Failed to write {kind} score file {path:?}: {error}");
        }
        ScoreStoreWriteError::CommitFile {
            path,
            tmp_path: _,
            error,
        } => {
            warn!("Failed to commit {kind} score file {path:?}: {error}");
        }
    }
}

#[derive(Debug, Default)]
pub struct ScoreCacheWarmupResult {
    pub results: Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)>,
    pub elapsed_ms: f64,
}

#[derive(Debug)]
pub struct ScoreCacheAccess<T> {
    pub value: T,
    pub results: Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)>,
}

type ProfilePathsFn = fn(&str) -> ScoreProfilePaths;

fn unix_time_ms_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn runtime_ensure_profile_score_caches_loaded(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    if profile_id.trim().is_empty() {
        return Vec::new();
    }
    vec![
        (
            ScoreCacheRuntimeKind::Local,
            runtime_ensure_local_score_cache_loaded(profile_id, score_paths),
        ),
        (
            ScoreCacheRuntimeKind::GrooveStats,
            runtime_ensure_gs_score_cache_loaded(profile_id, score_paths),
        ),
        (
            ScoreCacheRuntimeKind::ArrowCloud,
            runtime_ensure_ac_score_cache_loaded(profile_id, score_paths),
        ),
    ]
}

pub fn runtime_prewarm_select_music_score_caches(
    p1_profile_id: Option<&str>,
    p2_profile_id: Option<&str>,
    profiles: &[LocalScoreProfileSource],
    score_paths: ProfilePathsFn,
) -> ScoreCacheWarmupResult {
    let started = Instant::now();
    let mut results = Vec::new();

    if let Some(profile_id) = p1_profile_id {
        results.extend(runtime_ensure_profile_score_caches_loaded(
            profile_id,
            score_paths,
        ));
    }
    if let Some(profile_id) = p2_profile_id
        && p1_profile_id != Some(profile_id)
    {
        results.extend(runtime_ensure_profile_score_caches_loaded(
            profile_id,
            score_paths,
        ));
    }

    results.push((
        ScoreCacheRuntimeKind::MachineLocal,
        runtime_ensure_machine_local_score_cache_loaded(profiles),
    ));

    ScoreCacheWarmupResult {
        results,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    }
}

pub fn runtime_ensure_gs_score_cache_loaded(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    if RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_is_loaded(profile_id)
    {
        return ScoreCacheRuntimeResult::default();
    }

    let load_started = Instant::now();
    let loaded = load_gs_score_cache_from_paths(&score_paths(profile_id));
    let loaded_entries = loaded.by_chart.len();
    let elapsed_ms = load_started.elapsed().as_secs_f64() * 1000.0;

    let mut result = ScoreCacheRuntimeResult {
        load_report: Some(ScoreCacheLoadReport {
            kind: ScoreCacheLoadKind::GrooveStats,
            profile_id: Some(profile_id.to_string()),
            primary_entries: loaded_entries,
            secondary_entries: 0,
            elapsed_ms,
        }),
        write_errors: Vec::new(),
    };
    if let Some(error) = loaded.write_error {
        result.write_errors.push(error);
    }

    RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, loaded.by_chart);
    result
}

pub fn runtime_get_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> (Option<CachedScore>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_gs_score_cache_loaded(profile_id, score_paths);
    let score = RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_score(profile_id, chart_hash);
    (score, result)
}

pub fn runtime_read_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<CachedScore>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }
    let (value, result) = runtime_get_gs_score_for_profile(profile_id, chart_hash, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::GrooveStats, result)],
    }
}

pub fn runtime_gs_chart_hashes_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> (HashSet<String>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_gs_score_cache_loaded(profile_id, score_paths);
    let hashes = RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_chart_hashes(profile_id);
    (hashes, result)
}

pub fn runtime_read_gs_chart_hashes_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<HashSet<String>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: HashSet::new(),
            results: Vec::new(),
        };
    }
    let (value, result) = runtime_gs_chart_hashes_for_profile(profile_id, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::GrooveStats, result)],
    }
}

pub fn runtime_set_gs_score_for_profile(
    profile_id: &str,
    chart_hash: String,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    let mut result = runtime_ensure_gs_score_cache_loaded(profile_id, score_paths);
    let Some(snapshot) = RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .set_profile_score(profile_id, chart_hash, score)
    else {
        return result;
    };
    if let Err(error) =
        save_gs_score_index_file(&score_paths(profile_id).gs_index_path(), &snapshot)
    {
        result.write_errors.push(error);
    }
    result
}

pub fn runtime_write_gs_score_for_profile(
    profile_id: &str,
    chart_hash: String,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    vec![(
        ScoreCacheRuntimeKind::GrooveStats,
        runtime_set_gs_score_for_profile(
            profile_id,
            chart_hash,
            fix_gs_cached_score(score),
            score_paths,
        ),
    )]
}

#[derive(Debug)]
pub struct RuntimeGsScoreCacheResult {
    pub cache_result: ScoreCacheRuntimeResult,
    pub store_status: Option<ScoreStoreWriteStatus>,
    pub store_error: Option<ScoreStoreWriteError>,
    pub replaced: bool,
}

pub fn runtime_cache_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
    proves_nonquint_ex: bool,
    fetched_at_ms: i64,
    score_paths: ProfilePathsFn,
) -> RuntimeGsScoreCacheResult {
    let score = fix_gs_cached_score(score);
    let (existing, mut cache_result) =
        runtime_get_gs_score_for_profile(profile_id, chart_hash, score_paths);
    if let Some(existing) = existing
        && !should_replace_cached_gs_score(&score, &existing, proves_nonquint_ex)
    {
        return RuntimeGsScoreCacheResult {
            cache_result,
            store_status: None,
            store_error: None,
            replaced: false,
        };
    }

    let set_result =
        runtime_set_gs_score_for_profile(profile_id, chart_hash.to_string(), score, score_paths);
    cache_result.write_errors.extend(set_result.write_errors);
    if cache_result.load_report.is_none() {
        cache_result.load_report = set_result.load_report;
    }

    let (store_status, store_error) = if username.trim().is_empty() {
        (None, None)
    } else {
        match write_gs_score_entry_file(
            &score_paths(profile_id).gs_chart_dir(chart_hash),
            chart_hash,
            score,
            username,
            fetched_at_ms,
        ) {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error)),
        }
    };

    RuntimeGsScoreCacheResult {
        cache_result,
        store_status,
        store_error,
        replaced: true,
    }
}

pub fn runtime_cache_logged_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
    proves_nonquint_ex: bool,
    score_paths: ProfilePathsFn,
) {
    let result = runtime_cache_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        username,
        proves_nonquint_ex,
        unix_time_ms_now(),
        score_paths,
    );
    log_score_cache_result("GS", result.cache_result);
    if let Some(error) = result.store_error {
        log_score_store_write_error("GrooveStats", chart_hash, error);
    }
    if let Some(ScoreStoreWriteStatus::Written(path)) = result.store_status {
        debug!("Stored GrooveStats score on disk for chart {chart_hash} at {path:?}");
    }
}

pub fn runtime_seed_gs_score(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    let result = runtime_ensure_gs_score_cache_loaded(profile_id, score_paths);
    RUNTIME_GS_SCORE_CACHE
        .lock()
        .unwrap()
        .seed_profile_score(profile_id, chart_hash, score);
    result
}

pub fn runtime_seed_gs_score_access(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    vec![(
        ScoreCacheRuntimeKind::GrooveStats,
        runtime_seed_gs_score(profile_id, chart_hash, score, score_paths),
    )]
}

pub fn runtime_ensure_ac_score_cache_loaded(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    if RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_is_loaded(profile_id)
    {
        return ScoreCacheRuntimeResult::default();
    }
    let disk_cache = load_ac_score_index_for_profile(&score_paths(profile_id));
    RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, disk_cache);
    ScoreCacheRuntimeResult::default()
}

pub fn runtime_get_ac_scores_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> (Option<ArrowCloudScores>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_ac_score_cache_loaded(profile_id, score_paths);
    let scores = RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_scores(profile_id, chart_hash);
    (scores, result)
}

pub fn runtime_read_ac_scores_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<ArrowCloudScores>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }
    let (value, result) = runtime_get_ac_scores_for_profile(profile_id, chart_hash, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::ArrowCloud, result)],
    }
}

pub fn runtime_ac_chart_hashes_with_itg_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> (HashSet<String>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_ac_score_cache_loaded(profile_id, score_paths);
    let hashes = RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_chart_hashes_with_itg(profile_id);
    (hashes, result)
}

pub fn runtime_read_ac_chart_hashes_with_itg_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<HashSet<String>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: HashSet::new(),
            results: Vec::new(),
        };
    }
    let (value, result) = runtime_ac_chart_hashes_with_itg_for_profile(profile_id, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::ArrowCloud, result)],
    }
}

pub fn runtime_set_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    let mut result = runtime_ensure_ac_score_cache_loaded(profile_id, score_paths);
    let Some(snapshot) = RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .set_profile_scores_bulk(profile_id, entries)
    else {
        return result;
    };
    if let Err(error) =
        save_ac_score_index_file(&score_paths(profile_id).ac_index_path(), &snapshot)
    {
        result.write_errors.push(error);
    }
    result
}

pub fn runtime_write_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    vec![(
        ScoreCacheRuntimeKind::ArrowCloud,
        runtime_set_ac_scores_for_profile_bulk(profile_id, entries, score_paths),
    )]
}

pub fn runtime_merge_ac_submit_scores(
    profile_id: &str,
    chart_hash: &str,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    submitted_at: DateTime<Utc>,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    let mut result = runtime_ensure_ac_score_cache_loaded(profile_id, score_paths);
    let Some(snapshot) = RUNTIME_AC_SCORE_CACHE
        .lock()
        .unwrap()
        .merge_profile_submit_scores(
            profile_id,
            chart_hash,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
            submitted_at,
        )
    else {
        return result;
    };
    if let Err(error) =
        save_ac_score_index_file(&score_paths(profile_id).ac_index_path(), &snapshot)
    {
        result.write_errors.push(error);
    }
    result
}

pub fn runtime_write_ac_submit_scores(
    profile_id: &str,
    chart_hash: &str,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    submitted_at: DateTime<Utc>,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    vec![(
        ScoreCacheRuntimeKind::ArrowCloud,
        runtime_merge_ac_submit_scores(
            profile_id,
            chart_hash,
            itg_percent,
            ex_percent,
            hard_ex_percent,
            is_fail,
            submitted_at,
            score_paths,
        ),
    )]
}

pub fn runtime_read_logged_gs_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> Option<CachedScore> {
    let access = runtime_read_gs_score_for_profile(profile_id, chart_hash, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_read_logged_gs_chart_hashes_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> HashSet<String> {
    let access = runtime_read_gs_chart_hashes_for_profile(profile_id, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_write_logged_gs_score_for_profile(
    profile_id: &str,
    chart_hash: String,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) {
    debug!("Caching GrooveStats score {score:?} for chart hash {chart_hash}");
    log_score_cache_results(runtime_write_gs_score_for_profile(
        profile_id,
        chart_hash,
        score,
        score_paths,
    ));
}

pub fn runtime_read_logged_ac_scores_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> Option<ArrowCloudScores> {
    let access = runtime_read_ac_scores_for_profile(profile_id, chart_hash, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_read_logged_ac_chart_hashes_with_itg_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> HashSet<String> {
    let access = runtime_read_ac_chart_hashes_with_itg_for_profile(profile_id, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_write_logged_ac_scores_for_profile_bulk(
    profile_id: &str,
    entries: impl IntoIterator<Item = (String, ArrowCloudScores)>,
    score_paths: ProfilePathsFn,
) {
    log_score_cache_results(runtime_write_ac_scores_for_profile_bulk(
        profile_id,
        entries,
        score_paths,
    ));
}

#[allow(clippy::too_many_arguments)]
pub fn runtime_write_logged_ac_submit_scores(
    profile_id: &str,
    chart_hash: &str,
    itg_percent: f64,
    ex_percent: f64,
    hard_ex_percent: f64,
    is_fail: bool,
    submitted_at: DateTime<Utc>,
    score_paths: ProfilePathsFn,
) {
    log_score_cache_results(runtime_write_ac_submit_scores(
        profile_id,
        chart_hash,
        itg_percent,
        ex_percent,
        hard_ex_percent,
        is_fail,
        submitted_at,
        score_paths,
    ));
}

pub fn runtime_ensure_local_score_cache_loaded(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    if RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .profile_is_loaded(profile_id)
    {
        return ScoreCacheRuntimeResult::default();
    }

    let load_started = Instant::now();
    let loaded = load_local_score_cache_from_paths(&score_paths(profile_id));
    let elapsed_ms = load_started.elapsed().as_secs_f64() * 1000.0;
    let result = ScoreCacheRuntimeResult {
        load_report: Some(ScoreCacheLoadReport {
            kind: ScoreCacheLoadKind::Local,
            profile_id: Some(profile_id.to_string()),
            primary_entries: loaded.best_itg_count,
            secondary_entries: loaded.best_ex_count,
            elapsed_ms,
        }),
        write_errors: Vec::new(),
    };

    RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .insert_loaded_profile(profile_id, loaded.index);
    result
}

pub fn runtime_get_local_itg_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> (Option<CachedScore>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_local_score_cache_loaded(profile_id, score_paths);
    let score = RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_itg_score(profile_id, chart_hash);
    (score, result)
}

pub fn runtime_read_local_itg_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<CachedScore>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }
    let (value, result) =
        runtime_get_local_itg_score_for_profile(profile_id, chart_hash, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::Local, result)],
    }
}

pub fn runtime_get_local_pass_rate_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> (Option<u32>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_local_score_cache_loaded(profile_id, score_paths);
    let pass_rate = RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_pass_rate(profile_id, chart_hash);
    (pass_rate, result)
}

pub fn runtime_read_local_pass_rate_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<u32>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }
    let (value, result) =
        runtime_get_local_pass_rate_for_profile(profile_id, chart_hash, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::Local, result)],
    }
}

pub fn runtime_get_local_scalar_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    hard_ex: bool,
    score_paths: ProfilePathsFn,
) -> (Option<LocalScalarScore>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_local_score_cache_loaded(profile_id, score_paths);
    let score = RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .get_profile_scalar_score(profile_id, chart_hash, hard_ex);
    (score, result)
}

pub fn runtime_read_local_scalar_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    hard_ex: bool,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<LocalScalarScore>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }
    let (value, result) =
        runtime_get_local_scalar_score_for_profile(profile_id, chart_hash, hard_ex, score_paths);
    ScoreCacheAccess {
        value,
        results: vec![(ScoreCacheRuntimeKind::Local, result)],
    }
}

pub fn runtime_read_best_itg_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> ScoreCacheAccess<Option<CachedScore>> {
    if profile_id.trim().is_empty() {
        return ScoreCacheAccess {
            value: None,
            results: Vec::new(),
        };
    }

    let (local, local_result) =
        runtime_get_local_itg_score_for_profile(profile_id, chart_hash, score_paths);
    let (gs, gs_result) = runtime_get_gs_score_for_profile(profile_id, chart_hash, score_paths);
    let (ac, ac_result) = runtime_get_ac_scores_for_profile(profile_id, chart_hash, score_paths);
    let ac = ac
        .and_then(|scores| scores.itg)
        .map(|score| score.to_cached_score());
    ScoreCacheAccess {
        value: best_cached_itg_score([local, gs, ac]),
        results: vec![
            (ScoreCacheRuntimeKind::Local, local_result),
            (ScoreCacheRuntimeKind::GrooveStats, gs_result),
            (ScoreCacheRuntimeKind::ArrowCloud, ac_result),
        ],
    }
}

pub fn runtime_update_local_score_cache_after_append(
    profile_id: &str,
    chart_hash: &str,
    header: &LocalScoreHeader,
) -> Option<LocalScoreIndex> {
    RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .update_loaded_profile_index(profile_id, chart_hash, header)
}

#[derive(Debug)]
pub struct RuntimeLocalScoreAppendResult {
    pub append: Option<LocalScoreAppendRecord>,
    pub store_error: Option<ScoreStoreWriteError>,
    pub index_error: Option<ScoreIndexWriteError>,
}

pub fn runtime_append_local_score_for_profile(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
    score_paths: ProfilePathsFn,
) -> RuntimeLocalScoreAppendResult {
    let paths = score_paths(profile_id);
    let append = match write_local_score_entry_for_profile(&paths, chart_hash, entry) {
        Ok(append) => append,
        Err(error) => {
            return RuntimeLocalScoreAppendResult {
                append: None,
                store_error: Some(error),
                index_error: None,
            };
        }
    };

    let loaded_snapshot =
        runtime_update_local_score_cache_after_append(profile_id, chart_hash, &append.header);
    let index_error = save_local_score_index_after_append(
        &paths,
        chart_hash,
        &append.header,
        loaded_snapshot.as_ref(),
    )
    .err();

    runtime_update_machine_cache_if_loaded(chart_hash, append.cached_score, profile_initials);
    RuntimeLocalScoreAppendResult {
        append: Some(append),
        store_error: None,
        index_error,
    }
}

pub fn runtime_append_logged_local_score_for_profile(
    profile_id: &str,
    profile_initials: &str,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
    score_paths: ProfilePathsFn,
) -> bool {
    let result = runtime_append_local_score_for_profile(
        profile_id,
        profile_initials,
        chart_hash,
        entry,
        score_paths,
    );
    if let Some(error) = result.store_error {
        log_score_store_write_error("local", chart_hash, error);
    }
    if let Some(error) = result.index_error {
        log_score_index_write_error("local", error);
    }
    result.append.is_some()
}

pub fn runtime_seed_local_itg_score(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> ScoreCacheRuntimeResult {
    let result = runtime_ensure_local_score_cache_loaded(profile_id, score_paths);
    RUNTIME_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .seed_profile_itg_score(profile_id, chart_hash, score);
    result
}

pub fn runtime_seed_local_itg_score_access(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) -> Vec<(ScoreCacheRuntimeKind, ScoreCacheRuntimeResult)> {
    vec![(
        ScoreCacheRuntimeKind::Local,
        runtime_seed_local_itg_score(profile_id, chart_hash, score, score_paths),
    )]
}

pub fn runtime_prewarm_logged_select_music_score_caches(
    p1_profile_id: Option<&str>,
    p2_profile_id: Option<&str>,
    profiles: &[LocalScoreProfileSource],
    score_paths: ProfilePathsFn,
) {
    let warmup = runtime_prewarm_select_music_score_caches(
        p1_profile_id,
        p2_profile_id,
        profiles,
        score_paths,
    );
    log_score_cache_results(warmup.results);
    let elapsed_ms = warmup.elapsed_ms;
    debug!("Prewarmed SelectMusic score caches in {elapsed_ms:.2}ms.");
}

pub fn runtime_read_logged_local_itg_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> Option<CachedScore> {
    let access = runtime_read_local_itg_score_for_profile(profile_id, chart_hash, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_read_logged_local_pass_rate_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> Option<u32> {
    let access = runtime_read_local_pass_rate_for_profile(profile_id, chart_hash, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_read_logged_local_scalar_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    hard_ex: bool,
    score_paths: ProfilePathsFn,
) -> Option<LocalScalarScore> {
    let access =
        runtime_read_local_scalar_score_for_profile(profile_id, chart_hash, hard_ex, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_read_logged_best_itg_score_for_profile(
    profile_id: &str,
    chart_hash: &str,
    score_paths: ProfilePathsFn,
) -> Option<CachedScore> {
    let access = runtime_read_best_itg_score_for_profile(profile_id, chart_hash, score_paths);
    log_score_cache_results(access.results);
    access.value
}

pub fn runtime_ensure_logged_profile_score_caches_loaded(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) {
    log_score_cache_results(runtime_ensure_profile_score_caches_loaded(
        profile_id,
        score_paths,
    ));
}

pub fn runtime_seed_logged_local_itg_score(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) {
    log_score_cache_results(runtime_seed_local_itg_score_access(
        profile_id,
        chart_hash,
        score,
        score_paths,
    ));
}

pub fn runtime_seed_logged_gs_score(
    profile_id: &str,
    chart_hash: &str,
    score: CachedScore,
    score_paths: ProfilePathsFn,
) {
    log_score_cache_results(runtime_seed_gs_score_access(
        profile_id,
        chart_hash,
        score,
        score_paths,
    ));
}

pub fn runtime_machine_record_logged_local(
    chart_hash: &str,
    profiles: &[LocalScoreProfileSource],
) -> Option<(String, CachedScore)> {
    let (record, result) = runtime_machine_record_local(chart_hash, profiles);
    log_score_cache_result("local", result);
    record
}

pub fn runtime_total_songs_played_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> u32 {
    total_local_score_bins_in_root(&score_paths(profile_id).local_dir())
}

pub fn runtime_recent_played_chart_hashes_for_machine(profiles_root: &Path) -> Vec<String> {
    recent_played_chart_hashes_in_profiles_root(profiles_root)
}

pub fn runtime_played_chart_counts_for_machine(profiles_root: &Path) -> Vec<(String, u32)> {
    played_chart_counts_in_profiles_root(profiles_root)
}

pub fn runtime_recent_played_chart_hashes_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> Vec<String> {
    recent_played_chart_hashes_in_root(&score_paths(profile_id).local_dir())
}

pub fn runtime_played_chart_counts_for_profile(
    profile_id: &str,
    score_paths: ProfilePathsFn,
) -> Vec<(String, u32)> {
    played_chart_counts_in_root(&score_paths(profile_id).local_dir())
}

pub fn runtime_ensure_machine_local_score_cache_loaded(
    profiles: &[LocalScoreProfileSource],
) -> ScoreCacheRuntimeResult {
    let needs_load = {
        let state = RUNTIME_MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
        !state.loaded
    };
    if !needs_load {
        return ScoreCacheRuntimeResult::default();
    }

    let load_started = Instant::now();
    let best_itg = machine_best_itg_from_profiles(profiles);
    let mut state = RUNTIME_MACHINE_LOCAL_SCORE_CACHE.lock().unwrap();
    if state.loaded {
        return ScoreCacheRuntimeResult::default();
    }

    state.loaded = true;
    state.best_itg = best_itg;
    ScoreCacheRuntimeResult {
        load_report: Some(ScoreCacheLoadReport {
            kind: ScoreCacheLoadKind::MachineLocal,
            profile_id: None,
            primary_entries: state.best_itg.len(),
            secondary_entries: 0,
            elapsed_ms: load_started.elapsed().as_secs_f64() * 1000.0,
        }),
        write_errors: Vec::new(),
    }
}

pub fn runtime_update_machine_cache_if_loaded(
    chart_hash: &str,
    score: CachedScore,
    initials: &str,
) {
    RUNTIME_MACHINE_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .update_if_loaded(chart_hash, score, initials);
}

pub fn runtime_machine_record_local(
    chart_hash: &str,
    profiles: &[LocalScoreProfileSource],
) -> (Option<(String, CachedScore)>, ScoreCacheRuntimeResult) {
    let result = runtime_ensure_machine_local_score_cache_loaded(profiles);
    let record = RUNTIME_MACHINE_LOCAL_SCORE_CACHE
        .lock()
        .unwrap()
        .record(chart_hash);
    (record, result)
}

pub fn runtime_lock_score_caches() -> HeldScoreCaches {
    HeldScoreCaches::new(
        RUNTIME_LOCAL_SCORE_CACHE.lock().unwrap(),
        RUNTIME_GS_SCORE_CACHE.lock().unwrap(),
        RUNTIME_AC_SCORE_CACHE.lock().unwrap(),
    )
}

pub fn best_cached_itg_score(
    scores: impl IntoIterator<Item = Option<CachedScore>>,
) -> Option<CachedScore> {
    scores.into_iter().flatten().reduce(|a, b| {
        failed_score_override(&a, &b).unwrap_or_else(|| if is_better_itg(&a, &b) { a } else { b })
    })
}

pub struct HeldScoreCaches {
    local: MutexGuard<'static, LocalScoreCacheState>,
    gs: MutexGuard<'static, GsScoreCacheState>,
    ac: MutexGuard<'static, AcScoreCacheState>,
}

impl HeldScoreCaches {
    pub fn new(
        local: MutexGuard<'static, LocalScoreCacheState>,
        gs: MutexGuard<'static, GsScoreCacheState>,
        ac: MutexGuard<'static, AcScoreCacheState>,
    ) -> Self {
        Self { local, gs, ac }
    }

    /// Resolve the merged "best ITG" score for `chart_hash` under `profile_id`.
    pub fn merged(&self, profile_id: &str, chart_hash: &str) -> Option<CachedScore> {
        if profile_id.trim().is_empty() {
            return None;
        }
        let local = self
            .local
            .loaded_profiles
            .get(profile_id)
            .and_then(|idx| idx.best_itg.get(chart_hash).copied());
        let gs = self
            .gs
            .loaded_profiles
            .get(profile_id)
            .and_then(|m| m.get(chart_hash).copied());
        let ac = self
            .ac
            .loaded_profiles
            .get(profile_id)
            .and_then(|m| m.get(chart_hash).copied())
            .and_then(|s| s.itg)
            .map(|ac| ac.to_cached_score());
        best_cached_itg_score([local, gs, ac])
    }

    /// Snapshot every merged "best ITG" score for a loaded profile.
    ///
    /// Entries are sorted by chart hash so prepared runtime views can use
    /// binary search without rebuilding a map in the presentation layer.
    pub fn merged_profile_scores(&self, profile_id: &str) -> Vec<(String, CachedScore)> {
        if profile_id.trim().is_empty() {
            return Vec::new();
        }
        let local = self.local.loaded_profiles.get(profile_id);
        let gs = self.gs.loaded_profiles.get(profile_id);
        let ac = self.ac.loaded_profiles.get(profile_id);
        let capacity = local.map_or(0, |idx| idx.best_itg.len())
            + gs.map_or(0, HashMap::len)
            + ac.map_or(0, HashMap::len);
        let mut merged = HashMap::with_capacity(capacity);
        let mut insert = |chart_hash: &str, score: CachedScore| {
            merged
                .entry(chart_hash.to_string())
                .and_modify(|best| {
                    *best = best_cached_itg_score([Some(*best), Some(score)])
                        .expect("two scores always produce a best score");
                })
                .or_insert(score);
        };
        if let Some(index) = local {
            for (chart_hash, score) in &index.best_itg {
                insert(chart_hash, *score);
            }
        }
        if let Some(scores) = gs {
            for (chart_hash, score) in scores {
                insert(chart_hash, *score);
            }
        }
        if let Some(scores) = ac {
            for (chart_hash, score) in scores {
                if let Some(score) = score.itg {
                    insert(chart_hash, score.to_cached_score());
                }
            }
        }
        let mut merged: Vec<_> = merged.into_iter().collect();
        merged.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        merged
    }
}

/// Maximum number of attempts before the backoff schedule saturates. For
/// auto-retryable statuses this is also the auto-retry budget. For manual-only
/// statuses the cooldown caps here and stays there for subsequent failures.
pub const SUBMIT_RETRY_MAX_ATTEMPTS: u8 = 5;

/// Exponential backoff schedule used by every score-submission backend.
/// `attempt` is 1-based: 1 -> 2s, 2 -> 4s, 3 -> 8s, 4 -> 16s, 5 -> 32s.
#[inline(always)]
pub const fn submit_retry_delay_secs(attempt: u8) -> u64 {
    1u64 << attempt
}

/// Convert a remaining duration into whole seconds, rounding up so UI
/// countdowns show the configured delay instead of truncating immediately.
#[inline]
pub fn duration_to_ceil_secs(remaining: Duration) -> u32 {
    let secs = remaining.as_secs();
    let bumped = secs.saturating_add(if remaining.subsec_nanos() > 0 { 1 } else { 0 });
    bumped.min(u32::MAX as u64) as u32
}

pub struct SubmitRetryState<T> {
    by_side: [Vec<T>; 2],
}

impl<T> Default for SubmitRetryState<T> {
    fn default() -> Self {
        Self {
            by_side: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl<T> SubmitRetryState<T> {
    #[inline(always)]
    pub fn entries(&self, side_index: usize) -> &[T] {
        &self.by_side[side_index.min(1)]
    }

    #[inline(always)]
    pub fn entries_mut(&mut self, side_index: usize) -> &mut Vec<T> {
        &mut self.by_side[side_index.min(1)]
    }

    pub fn reset_by_key<K>(&mut self, side_index: usize, chart_hash: &str, key: K)
    where
        K: Fn(&T) -> &str,
    {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        self.entries_mut(side_index)
            .retain(|entry| !key(entry).eq_ignore_ascii_case(hash));
    }

    pub fn upsert_by_key<K>(&mut self, side_index: usize, entry: T, key: K, cap: usize)
    where
        K: Fn(&T) -> &str,
    {
        let hash = key(&entry).trim().to_string();
        if hash.is_empty() {
            return;
        }
        let entries = self.entries_mut(side_index);
        if let Some(stored) = entries
            .iter_mut()
            .find(|stored| key(stored).eq_ignore_ascii_case(hash.as_str()))
        {
            *stored = entry;
            return;
        }
        entries.push(entry);
        if entries.len() > cap {
            entries.drain(0..entries.len() - cap);
        }
    }

    pub fn get_by_key<K>(&self, side_index: usize, chart_hash: &str, key: K) -> Option<&T>
    where
        K: Fn(&T) -> &str,
    {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return None;
        }
        self.entries(side_index)
            .iter()
            .find(|entry| key(entry).eq_ignore_ascii_case(hash))
    }

    pub fn get_mut_by_key<K>(
        &mut self,
        side_index: usize,
        chart_hash: &str,
        key: K,
    ) -> Option<&mut T>
    where
        K: Fn(&T) -> &str,
    {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return None;
        }
        self.entries_mut(side_index)
            .iter_mut()
            .find(|entry| key(entry).eq_ignore_ascii_case(hash))
    }

    pub fn take_ready_by_key<K, N>(
        &mut self,
        side_index: usize,
        chart_hash: &str,
        manual: bool,
        now: Instant,
        key: K,
        next_retry_at: N,
    ) -> Option<T>
    where
        T: Clone,
        K: Fn(&T) -> &str,
        N: Fn(&mut T) -> &mut Option<Instant>,
    {
        let stored = self.get_mut_by_key(side_index, chart_hash, key)?;
        let next = next_retry_at(stored);
        if manual && next.is_some_and(|t| t > now) {
            return None;
        }
        *next = None;
        Some(stored.clone())
    }

    pub fn record_failure_by_key<K, A, N>(
        &mut self,
        side_index: usize,
        chart_hash: &str,
        can_retry: bool,
        max_attempts: u8,
        now: Instant,
        key: K,
        retry_attempt: A,
        next_retry_at: N,
    ) -> bool
    where
        K: Fn(&T) -> &str,
        A: Fn(&mut T) -> &mut u8,
        N: Fn(&mut T) -> &mut Option<Instant>,
    {
        let Some(entry) = self.get_mut_by_key(side_index, chart_hash, key) else {
            return false;
        };
        if !can_retry {
            *next_retry_at(entry) = None;
            return true;
        }
        let attempt = retry_attempt(entry);
        *attempt = attempt.saturating_add(1).min(max_attempts);
        *next_retry_at(entry) = Some(now + Duration::from_secs(submit_retry_delay_secs(*attempt)));
        true
    }

    pub fn remaining_secs_by_key<K, N>(
        &self,
        side_index: usize,
        chart_hash: &str,
        now: Instant,
        key: K,
        next_retry_at: N,
    ) -> Option<u32>
    where
        K: Fn(&T) -> &str,
        N: Fn(&T) -> Option<Instant>,
    {
        let target = next_retry_at(self.get_by_key(side_index, chart_hash, key)?)?;
        Some(duration_to_ceil_secs(target.saturating_duration_since(now)))
    }

    pub fn retry_attempt_by_key<K, A>(
        &self,
        side_index: usize,
        chart_hash: &str,
        key: K,
        retry_attempt: A,
    ) -> Option<u8>
    where
        K: Fn(&T) -> &str,
        A: Fn(&T) -> u8,
    {
        self.get_by_key(side_index, chart_hash, key)
            .map(retry_attempt)
    }

    pub fn due_retries<K, S, A, N, Side>(
        &self,
        now: Instant,
        key: K,
        side: S,
        retry_attempt: A,
        next_retry_at: N,
    ) -> Vec<(String, Side, u8)>
    where
        K: Fn(&T) -> &str,
        S: Fn(&T) -> Side,
        A: Fn(&T) -> u8,
        N: Fn(&T) -> Option<Instant>,
        Side: Copy,
    {
        (0..2)
            .flat_map(|side_index| self.entries(side_index).iter())
            .filter_map(|entry| {
                next_retry_at(entry)
                    .filter(|t| *t <= now)
                    .map(|_| (key(entry).to_string(), side(entry), retry_attempt(entry)))
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
struct SubmitUiEntry<S> {
    chart_hash: String,
    token: u64,
    status: S,
}

pub struct SubmitUiState<S> {
    by_side: [Vec<SubmitUiEntry<S>>; 2],
}

impl<S> Default for SubmitUiState<S> {
    fn default() -> Self {
        Self {
            by_side: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl<S: Copy> SubmitUiState<S> {
    #[inline(always)]
    fn entries_mut(&mut self, side_index: usize) -> &mut Vec<SubmitUiEntry<S>> {
        &mut self.by_side[side_index.min(1)]
    }

    #[inline(always)]
    fn entries(&self, side_index: usize) -> &[SubmitUiEntry<S>] {
        &self.by_side[side_index.min(1)]
    }

    pub fn reset(&mut self, side_index: usize, chart_hash: &str) {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        self.entries_mut(side_index)
            .retain(|entry| !entry.chart_hash.eq_ignore_ascii_case(hash));
    }

    pub fn set(&mut self, side_index: usize, chart_hash: &str, token: u64, status: S) {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        let entries = self.entries_mut(side_index);
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        {
            entry.token = token;
            entry.status = status;
            return;
        }
        entries.push(SubmitUiEntry {
            chart_hash: hash.to_string(),
            token,
            status,
        });
    }

    pub fn update_if_token(
        &mut self,
        side_index: usize,
        chart_hash: &str,
        token: u64,
        status: S,
    ) -> bool {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return false;
        }
        let Some(entry) = self
            .entries_mut(side_index)
            .iter_mut()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        else {
            return false;
        };
        if entry.token != token {
            return false;
        }
        entry.status = status;
        true
    }

    pub fn get(&self, side_index: usize, chart_hash: &str) -> Option<S> {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return None;
        }
        self.entries(side_index)
            .iter()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
            .map(|entry| entry.status)
    }
}

#[derive(Debug, Clone)]
struct SubmitEventUiEntry<P, B> {
    chart_hash: String,
    token: u64,
    progress: P,
    banner: Option<B>,
}

pub struct SubmitEventUiState<P, B> {
    by_side: [Vec<SubmitEventUiEntry<P, B>>; 2],
}

impl<P, B> Default for SubmitEventUiState<P, B> {
    fn default() -> Self {
        Self {
            by_side: std::array::from_fn(|_| Vec::new()),
        }
    }
}

impl<P: Clone + Default, B: Clone> SubmitEventUiState<P, B> {
    #[inline(always)]
    fn entries_mut(&mut self, side_index: usize) -> &mut Vec<SubmitEventUiEntry<P, B>> {
        &mut self.by_side[side_index.min(1)]
    }

    #[inline(always)]
    fn entries(&self, side_index: usize) -> &[SubmitEventUiEntry<P, B>] {
        &self.by_side[side_index.min(1)]
    }

    pub fn reset(&mut self, side_index: usize, chart_hash: &str) {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        self.entries_mut(side_index)
            .retain(|entry| !entry.chart_hash.eq_ignore_ascii_case(hash));
    }

    pub fn arm(&mut self, side_index: usize, chart_hash: &str, token: u64) {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        let entries = self.entries_mut(side_index);
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        {
            entry.token = token;
            entry.progress = P::default();
            entry.banner = None;
            return;
        }
        entries.push(SubmitEventUiEntry {
            chart_hash: hash.to_string(),
            token,
            progress: P::default(),
            banner: None,
        });
    }

    pub fn update_if_token(
        &mut self,
        side_index: usize,
        chart_hash: &str,
        token: u64,
        progress: P,
        banner: Option<B>,
    ) {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return;
        }
        let Some(entry) = self
            .entries_mut(side_index)
            .iter_mut()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
        else {
            return;
        };
        if entry.token != token {
            return;
        }
        entry.progress = progress;
        entry.banner = banner;
    }

    pub fn progress(&self, side_index: usize, chart_hash: &str) -> P {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return P::default();
        }
        self.entries(side_index)
            .iter()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
            .map(|entry| entry.progress.clone())
            .unwrap_or_default()
    }

    pub fn banner(&self, side_index: usize, chart_hash: &str) -> Option<B> {
        let hash = chart_hash.trim();
        if hash.is_empty() {
            return None;
        }
        self.entries(side_index)
            .iter()
            .find(|entry| entry.chart_hash.eq_ignore_ascii_case(hash))
            .and_then(|entry| entry.banner.clone())
    }
}

/// Why a submitted score was rejected by the backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// The backend accepted the request but considers the score invalid.
    InvalidScore,
    /// HTTP 401 / 403: the API key was not accepted.
    Unauthorized,
    /// HTTP 404: the chart hash is unknown to the backend.
    NotFound,
}

impl RejectReason {
    /// Human-readable label suitable for the evaluation footer.
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvalidScore => "Invalid Score",
            Self::Unauthorized => "Unauthorized",
            Self::NotFound => "Unknown Chart",
        }
    }
}

/// Server-side grade name returned by ArrowCloud's grading systems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ArrowCloudServerGrade {
    Sex,
    Quint,
    Quad,
    Tristar,
    Twostar,
    Star,
    SPlus,
    S,
    SMinus,
    APlus,
    A,
    AMinus,
    BPlus,
    B,
    BMinus,
    CPlus,
    C,
    CMinus,
    D,
    Failed,
}

impl ArrowCloudServerGrade {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sex => "Sex",
            Self::Quint => "Quint",
            Self::Quad => "Quad",
            Self::Tristar => "Tristar",
            Self::Twostar => "Twostar",
            Self::Star => "Star",
            Self::SPlus => "S+",
            Self::S => "S",
            Self::SMinus => "S-",
            Self::APlus => "A+",
            Self::A => "A",
            Self::AMinus => "A-",
            Self::BPlus => "B+",
            Self::B => "B",
            Self::BMinus => "B-",
            Self::CPlus => "C+",
            Self::C => "C",
            Self::CMinus => "C-",
            Self::D => "D",
            Self::Failed => "F",
        }
    }

    /// Parse the canonical AC grade string. Case-sensitive; whitespace is trimmed.
    pub fn from_server_str(s: &str) -> Option<Self> {
        match s.trim() {
            "Sex" => Some(Self::Sex),
            "Quint" => Some(Self::Quint),
            "Quad" => Some(Self::Quad),
            "Tristar" => Some(Self::Tristar),
            "Twostar" => Some(Self::Twostar),
            "Star" => Some(Self::Star),
            "S+" => Some(Self::SPlus),
            "S" => Some(Self::S),
            "S-" => Some(Self::SMinus),
            "A+" => Some(Self::APlus),
            "A" => Some(Self::A),
            "A-" => Some(Self::AMinus),
            "B+" => Some(Self::BPlus),
            "B" => Some(Self::B),
            "B-" => Some(Self::BMinus),
            "C+" => Some(Self::CPlus),
            "C" => Some(Self::C),
            "C-" => Some(Self::CMinus),
            "D" => Some(Self::D),
            "F" => Some(Self::Failed),
            _ => None,
        }
    }
}

/// A single ArrowCloud score for one chart/leaderboard pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowCloudScore {
    /// 0.0..=1.0, from the API percent divided by 100.
    pub score_percent: f64,
    pub server_grade: Option<ArrowCloudServerGrade>,
    pub played_at: Option<DateTime<Utc>>,
    pub play_id: Option<i64>,
    pub is_fail: bool,
}

impl Encode for ArrowCloudScore {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> Result<(), bincode::error::EncodeError> {
        self.score_percent.encode(encoder)?;
        self.server_grade.encode(encoder)?;
        self.played_at
            .map(|d| d.timestamp_millis())
            .encode(encoder)?;
        self.play_id.encode(encoder)?;
        self.is_fail.encode(encoder)?;
        Ok(())
    }
}

impl<C> Decode<C> for ArrowCloudScore {
    fn decode<D: bincode::de::Decoder<Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        let score_percent = f64::decode(decoder)?;
        let server_grade = Option::<ArrowCloudServerGrade>::decode(decoder)?;
        let played_at_ms = Option::<i64>::decode(decoder)?;
        let play_id = Option::<i64>::decode(decoder)?;
        let is_fail = bool::decode(decoder)?;
        let played_at = played_at_ms.and_then(|ms| Utc.timestamp_millis_opt(ms).single());
        Ok(Self {
            score_percent,
            server_grade,
            played_at,
            play_id,
            is_fail,
        })
    }
}

impl<'de, C> bincode::BorrowDecode<'de, C> for ArrowCloudScore {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = C>>(
        decoder: &mut D,
    ) -> Result<Self, bincode::error::DecodeError> {
        Self::decode(decoder)
    }
}

impl ArrowCloudScore {
    /// Adapter used to merge AC cache entries with local and GrooveStats scores.
    pub fn to_cached_score(&self) -> CachedScore {
        let score_10000 = (self.score_percent * 10000.0).clamp(0.0, 10000.0);
        if self.is_fail {
            return CachedScore {
                grade: Grade::Failed,
                score_percent: score_10000 / 10000.0,
                lamp_index: None,
                lamp_judge_count: None,
            };
        }
        CachedScore {
            grade: score_to_grade(score_10000),
            score_percent: score_10000 / 10000.0,
            lamp_index: (((score_10000 / 10000.0) - 1.0).abs() <= 1e-9).then_some(1),
            lamp_judge_count: None,
        }
    }
}

/// Cached AC scores for a single chart, one entry per global leaderboard.
#[derive(Debug, Clone, Copy, Default, PartialEq, Encode, Decode)]
pub struct ArrowCloudScores {
    pub itg: Option<ArrowCloudScore>,
    pub ex: Option<ArrowCloudScore>,
    pub hard_ex: Option<ArrowCloudScore>,
}

pub fn arrowcloud_score_from_retrieve_fields(
    score: Option<f64>,
    grade: Option<&str>,
    date: Option<&str>,
    play_id: Option<i64>,
    is_fail: bool,
) -> Option<ArrowCloudScore> {
    let percent_0_100 = score?.clamp(0.0, 100.0);
    let server_grade = grade.and_then(ArrowCloudServerGrade::from_server_str);
    let played_at = date
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    Some(ArrowCloudScore {
        score_percent: percent_0_100 / 100.0,
        server_grade,
        played_at,
        play_id,
        is_fail,
    })
}

pub fn arrowcloud_score_from_submit_percent(
    percent_0_100: f64,
    is_fail: bool,
    submitted_at: DateTime<Utc>,
) -> Option<ArrowCloudScore> {
    if !percent_0_100.is_finite() {
        return None;
    }
    Some(ArrowCloudScore {
        score_percent: percent_0_100.clamp(0.0, 100.0) / 100.0,
        server_grade: None,
        played_at: Some(submitted_at),
        play_id: None,
        is_fail,
    })
}

pub fn set_arrowcloud_score_for_leaderboard(
    scores: &mut ArrowCloudScores,
    leaderboard_id: u32,
    score: ArrowCloudScore,
) -> bool {
    match ArrowCloudLeaderboard::from_id(leaderboard_id) {
        Some(ArrowCloudLeaderboard::Itg) => scores.itg = Some(score),
        Some(ArrowCloudLeaderboard::Ex) => scores.ex = Some(score),
        Some(ArrowCloudLeaderboard::HardEx) => scores.hard_ex = Some(score),
        None => return false,
    }
    true
}

#[inline]
pub fn merge_arrowcloud_score_slot(
    existing: &mut Option<ArrowCloudScore>,
    incoming: Option<ArrowCloudScore>,
) {
    let Some(new_score) = incoming else {
        return;
    };
    match existing {
        None => *existing = Some(new_score),
        Some(prev) => {
            if prev.is_fail && !new_score.is_fail {
                *prev = new_score;
            } else if !prev.is_fail && new_score.is_fail {
                // Keep prev: the existing non-failed score wins.
            } else if new_score.score_percent > prev.score_percent {
                *prev = new_score;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ArrowCloudSubmitStats {
    pub judgment_counts: judgment::JudgeCounts,
    pub window_counts: timing::WindowCounts,
    pub holds_held: u32,
    pub mines_hit: u32,
    pub mines_avoided: u32,
    pub rolls_held: u32,
}

#[inline(always)]
pub fn arrowcloud_time_in_submit_window(time_ns: i64, fail_time_ns: Option<i64>) -> bool {
    match fail_time_ns {
        Some(fail_time) => !song_time_ns_invalid(time_ns) && time_ns <= fail_time,
        None => true,
    }
}

pub fn arrowcloud_submit_stats_from_results(
    notes: &[Note],
    note_times: &[i64],
    hold_end_times: &[Option<i64>],
    fail_time_ns: Option<i64>,
) -> ArrowCloudSubmitStats {
    let mut stats = ArrowCloudSubmitStats::default();
    let mut idx = 0usize;
    while idx < notes.len() {
        let row_index = notes[idx].row_index;
        let row_start = idx;
        let row_time = note_times.get(idx).copied().unwrap_or(i64::MIN);
        while idx < notes.len() && notes[idx].row_index == row_index {
            idx += 1;
        }
        if !arrowcloud_time_in_submit_window(row_time, fail_time_ns) {
            continue;
        }
        let Some(row_judgment) =
            judgment::aggregate_row_final_judgment(notes[row_start..idx].iter().filter_map(|n| {
                if n.is_fake || !n.can_be_judged || matches!(n.note_type, NoteType::Mine) {
                    None
                } else {
                    n.result.as_ref()
                }
            }))
        else {
            continue;
        };
        stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)] =
            stats.judgment_counts[judgment::judge_grade_ix(row_judgment.grade)].saturating_add(1);
        judgment::add_judgment_to_window_counts(
            &mut stats.window_counts,
            row_judgment,
            timing::FA_PLUS_W0_MS,
        );
    }

    for (i, note) in notes.iter().enumerate() {
        if note.is_fake || !note.can_be_judged {
            continue;
        }
        let note_time = note_times.get(i).copied().unwrap_or(i64::MIN);
        match note.note_type {
            NoteType::Hold | NoteType::Roll => {
                let result_time = hold_end_times
                    .get(i)
                    .and_then(|time| *time)
                    .unwrap_or(note_time);
                if !arrowcloud_time_in_submit_window(result_time, fail_time_ns) {
                    continue;
                }
                if note.hold.as_ref().and_then(|h| h.result) == Some(HoldResult::Held) {
                    if note.note_type == NoteType::Hold {
                        stats.holds_held = stats.holds_held.saturating_add(1);
                    } else {
                        stats.rolls_held = stats.rolls_held.saturating_add(1);
                    }
                }
            }
            NoteType::Mine => {
                if !arrowcloud_time_in_submit_window(note_time, fail_time_ns) {
                    continue;
                }
                match note.mine_result {
                    Some(MineResult::Hit) => {
                        stats.mines_hit = stats.mines_hit.saturating_add(1);
                    }
                    Some(MineResult::Avoided) => {
                        stats.mines_avoided = stats.mines_avoided.saturating_add(1);
                    }
                    None => {}
                }
            }
            NoteType::Tap | NoteType::Lift | NoteType::Fake => {}
        }
    }

    stats
}

pub fn arrowcloud_submit_stats_from_live_or_results(
    live_stats: ArrowCloudSubmitStats,
    fail_time_ns: Option<i64>,
    notes: &[Note],
    note_times: &[i64],
    hold_end_times: &[Option<i64>],
) -> ArrowCloudSubmitStats {
    match fail_time_ns {
        None => live_stats,
        Some(fail_time_ns) => arrowcloud_submit_stats_from_results(
            notes,
            note_times,
            hold_end_times,
            Some(fail_time_ns),
        ),
    }
}

/// Global ArrowCloud leaderboard variants. Numeric values match server IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum ArrowCloudLeaderboard {
    Ex = 2,
    Itg = 3,
    HardEx = 4,
}

impl ArrowCloudLeaderboard {
    pub const ALL_GLOBAL: [Self; 3] = [Self::HardEx, Self::Ex, Self::Itg];

    #[inline(always)]
    pub const fn id(self) -> u32 {
        self as u32
    }

    #[inline(always)]
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            2 => Some(Self::Ex),
            3 => Some(Self::Itg),
            4 => Some(Self::HardEx),
            _ => None,
        }
    }
}

impl Serialize for ArrowCloudLeaderboard {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.id())
    }
}

// Lua charts stay blocked from online submit unless their effects have been
// verified closely enough to match ITGmania for scoring purposes.
const LUA_SCORE_SUBMIT_ALLOWLIST: &[&str] = &[
    // "d5bd4dd7224f68ff", // Spooky (SM)
    // "c9e45c5e534f058d", // media offline (SM)
    // "596b42ed8317d9b8", // Godspeed (SX)
    // "3926ec3e5f1aaede", // CO5M1C R4ILR0AD (SH)
    // "a147dd828cd08fc7", // Riddle (DX)
    // "f95bc209c6f2cbfe", // Levels (SM)
    // "b50d0c3916e75b84", // Levels (SH)
    // "f41a24722a37758f", // Levels (SX)
];

#[inline(always)]
pub fn lua_chart_submit_allowed(chart_hash: &str) -> bool {
    let hash = chart_hash.trim();
    !hash.is_empty()
        && LUA_SCORE_SUBMIT_ALLOWLIST
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(hash))
}

#[inline(always)]
pub fn lua_submit_allowed(song_has_lua: bool, chart_hash: &str) -> bool {
    !song_has_lua || lua_chart_submit_allowed(chart_hash)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GsCommentCounts {
    pub w: u32,
    pub e: u32,
    pub g: u32,
    pub d: u32,
    pub wo: u32,
    pub m: u32,
}

pub fn parse_gs_comment_counts(comment: &str) -> GsCommentCounts {
    let mut counts = GsCommentCounts::default();
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

        match s[idx..].trim().as_bytes() {
            [b'w' | b'W'] => counts.w = value,
            [b'e' | b'E'] => counts.e = value,
            [b'g' | b'G'] => counts.g = value,
            [b'd' | b'D'] => counts.d = value,
            [b'w' | b'W', b'o' | b'O'] => counts.wo = value,
            [b'm' | b'M'] => counts.m = value,
            _ => {}
        }
    }
    counts
}

pub fn parse_gs_comment_ex_percent(comment: &str) -> Option<f64> {
    let bytes = comment.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        if !bytes[idx].is_ascii_digit() {
            idx += 1;
            continue;
        }

        let start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if idx < bytes.len() && bytes[idx] == b'.' {
            idx += 1;
            while idx < bytes.len() && bytes[idx].is_ascii_digit() {
                idx += 1;
            }
        }

        let value_end = idx;
        if idx < bytes.len() && bytes[idx] == b'%' {
            idx += 1;
        }
        while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
            idx += 1;
        }

        if idx + 2 <= bytes.len()
            && bytes[idx].eq_ignore_ascii_case(&b'e')
            && bytes[idx + 1].eq_ignore_ascii_case(&b'x')
            && bytes
                .get(idx + 2)
                .is_none_or(|next| !next.is_ascii_alphabetic())
        {
            return comment[start..value_end].parse::<f64>().ok();
        }
    }
    None
}

#[inline(always)]
pub fn groovestats_score_10000_from_counts(
    scoring_counts: &deadsync_rules::judgment::JudgeCounts,
    holds_held_for_score: u32,
    rolls_held_for_score: u32,
    mines_hit_for_score: u32,
    possible_grade_points: i32,
) -> u32 {
    let score_percent = deadsync_rules::judgment::calculate_itg_score_percent_from_counts(
        scoring_counts,
        holds_held_for_score,
        rolls_held_for_score,
        mines_hit_for_score,
        possible_grade_points,
    );
    (score_percent * 10000.0).round().clamp(0.0, 10000.0) as u32
}

#[inline(always)]
pub fn groovestats_rate_hundredths(music_rate: f32) -> u32 {
    if music_rate.is_finite() && music_rate > 0.0 {
        (music_rate * 100.0).round().clamp(0.0, u32::MAX as f32) as u32
    } else {
        100
    }
}

#[inline(always)]
pub const fn groovestats_used_cmod(scroll_speed: ScrollSpeedSetting) -> bool {
    matches!(scroll_speed, ScrollSpeedSetting::CMod(_))
}

#[inline(always)]
pub fn gs_ex_scoreboard_is_quint(score_10000: f64) -> bool {
    score_10000.is_finite() && score_10000.round().clamp(0.0, 10000.0) >= 10000.0
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GsExEvidence {
    pub leaderboard_score_10000: Option<f64>,
    pub comment_percent: Option<f64>,
}

impl GsExEvidence {
    pub fn from_sources(leaderboard_score_10000: Option<f64>, comment: Option<&str>) -> Self {
        Self {
            leaderboard_score_10000,
            comment_percent: comment.and_then(parse_gs_comment_ex_percent),
        }
    }

    #[inline(always)]
    pub fn is_quint(self) -> Option<bool> {
        if let Some(score) = self.leaderboard_score_10000 {
            return Some(gs_ex_scoreboard_is_quint(score));
        }
        self.comment_percent.map(|ex| ex >= 100.0)
    }

    #[inline(always)]
    pub fn proves_nonquint(self) -> bool {
        self.is_quint() == Some(false)
    }
}

pub fn gs_lamp_judge_count(lamp_index: Option<u8>, comment: Option<&str>) -> Option<u8> {
    let lamp_index = lamp_index?;
    let comment = comment?;
    let counts = parse_gs_comment_counts(comment);

    // zmod-style single-digit overlay:
    // - lamp 1 shows #white fantastics
    // - lamp 2 shows #W2
    // - lamp 3 shows #W3
    let count = match lamp_index {
        1 => counts.w,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GsLampChartStats {
    pub total_steps: u32,
    pub holds: u32,
    pub rolls: u32,
}

const GS_DP_W1: i32 = 5;
const GS_DP_W2: i32 = 4;
const GS_DP_W3: i32 = 2;
const GS_DP_W4: i32 = 0;
const GS_DP_W5: i32 = -6;
const GS_DP_MISS: i32 = -12;
const GS_DP_HELD: i32 = 5;

pub fn gs_lamp_index_from_chart_stats(
    score_10000: f64,
    comment: Option<&str>,
    ex_evidence: GsExEvidence,
    stats: Option<GsLampChartStats>,
) -> Option<u8> {
    let score_percent = score_10000 / 10000.0;

    // Perfect 100% ITG can still be a quad. Trust the GS/BS EX scoreboard
    // first; only fall back to the EX comment when that board is unavailable.
    if (score_percent - 1.0).abs() <= 1e-9 {
        if let Some(is_quint) = ex_evidence.is_quint() {
            return Some(if is_quint { 0 } else { 1 });
        }
        if let Some(comment) = comment {
            let counts = parse_gs_comment_counts(comment);
            let explicit_nonquint_counts = counts.w != 0
                || counts.e != 0
                || counts.g != 0
                || counts.d != 0
                || counts.wo != 0
                || counts.m != 0;
            if explicit_nonquint_counts {
                return Some(1);
            }
        }
        return Some(1);
    }

    let comment = comment?;
    let counts = parse_gs_comment_counts(comment);

    // Any explicit Miss or Way Off disqualifies lamps immediately.
    if counts.m > 0 || counts.wo > 0 {
        return None;
    }

    let stats = stats?;
    let taps_rows = stats.total_steps as i32;
    let holds = stats.holds as i32;
    let rolls = stats.rolls as i32;

    if taps_rows <= 0 {
        return None;
    }

    // Reconstruct W1 count as "everything not explicitly listed".
    let non_w1_from_suffixes = counts.e + counts.g + counts.d + counts.wo + counts.m + counts.w;
    let inferred_w1 = if (non_w1_from_suffixes as i32) > taps_rows {
        0
    } else {
        stats
            .total_steps
            .saturating_sub(counts.e + counts.g + counts.d + counts.wo + counts.m)
    };
    let w1_total = counts.w.max(inferred_w1);

    // Dance Points from tap judgments (rows) only, per ITG PercentScoreWeight.
    let dp_taps: i32 = (w1_total as i32) * GS_DP_W1
        + (counts.e as i32) * GS_DP_W2
        + (counts.g as i32) * GS_DP_W3
        + (counts.d as i32) * GS_DP_W4
        + (counts.wo as i32) * GS_DP_W5
        + (counts.m as i32) * GS_DP_MISS;

    // Holds + rolls assumed fully held for the "no hidden errors" hypothesis.
    let dp_hold_roll: i32 = (holds + rolls) * GS_DP_HELD;

    // Maximum possible DP if every tap was W1 and all holds/rolls fully held.
    let dp_possible_max: i32 = (taps_rows * GS_DP_W1 + dp_hold_roll).max(1);
    let dp_expect_no_hidden_errors: i32 = dp_taps + dp_hold_roll;

    let dp_expect_frac = f64::from(dp_expect_no_hidden_errors) / f64::from(dp_possible_max);
    let dp_diff = (score_percent - dp_expect_frac).abs();
    let dp_consistent = dp_diff <= 0.0005;

    if !dp_consistent {
        return None;
    }

    // At this point, we know there were no hidden hold/mine mistakes.
    // Classify the lamp tier, mirroring Simply Love's StageAward semantics.
    if counts.g == 0 && counts.d == 0 && counts.wo == 0 && counts.m == 0 {
        // Only W1/W2 present (and W1 reconstructed) => W2 full combo (FEC).
        if counts.e > 0 || w1_total > 0 {
            return Some(2);
        }
    }

    if counts.d == 0 && counts.wo == 0 && counts.m == 0 && counts.g > 0 {
        return Some(3);
    }

    // No WayOff/Miss and DP-consistent => at worst a W4 full combo.
    if counts.wo == 0 && counts.m == 0 {
        return Some(4);
    }

    None
}

#[inline(always)]
pub fn cached_failed_gs_score(score_10000: f64) -> CachedScore {
    cached_score(Grade::Failed, score_10000 / 10000.0, None, None)
}

#[inline(always)]
pub fn cached_missing_gs_score() -> CachedScore {
    cached_failed_gs_score(0.0)
}

#[inline(always)]
pub fn cached_gs_score_from_lamp(
    score_10000: f64,
    is_fail: bool,
    lamp_index: Option<u8>,
    comment: Option<&str>,
) -> CachedScore {
    if is_fail {
        return cached_failed_gs_score(score_10000);
    }
    let lamp_judge_count = gs_lamp_judge_count(lamp_index, comment);
    let mut grade = score_to_grade(score_10000);
    if lamp_index == Some(0) {
        grade = promote_quint_grade(grade, 100.0);
    }
    cached_score(grade, score_10000 / 10000.0, lamp_index, lamp_judge_count)
}

#[inline(always)]
pub fn cached_gs_score_from_chart_stats(
    score_10000: f64,
    is_fail: bool,
    comment: Option<&str>,
    ex_evidence: GsExEvidence,
    stats: Option<GsLampChartStats>,
) -> CachedScore {
    let lamp_index = gs_lamp_index_from_chart_stats(score_10000, comment, ex_evidence, stats);
    cached_gs_score_from_lamp(score_10000, is_fail, lamp_index, comment)
}

#[inline(always)]
pub fn score_file_shard(hash: &str) -> &str {
    hash.get(..2).unwrap_or("00")
}

pub fn parse_score_file_name(name: &str) -> Option<(&str, i64)> {
    let base = name.strip_suffix(".bin")?;
    let idx_dash = base.rfind('-')?;
    if idx_dash == 0 {
        return None;
    }
    let played_at_ms = base[(idx_dash + 1)..].parse::<i64>().ok()?;
    Some((&base[..idx_dash], played_at_ms))
}

#[inline(always)]
pub const fn gameplay_run_passed(
    song_completed_naturally: bool,
    is_failing: bool,
    life: f32,
    has_fail_time: bool,
) -> bool {
    song_completed_naturally && !gameplay_run_failed(is_failing, has_fail_time) && life > 0.0
}

#[inline(always)]
pub const fn gameplay_run_failed(is_failing: bool, has_fail_time: bool) -> bool {
    is_failing || has_fail_time
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudAutosubmitLogLevel {
    Debug,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrowCloudAutosubmitLog {
    pub level: ArrowCloudAutosubmitLogLevel,
    pub reason: &'static str,
}

impl ArrowCloudAutosubmitLog {
    pub const fn debug(reason: &'static str) -> Self {
        Self {
            level: ArrowCloudAutosubmitLogLevel::Debug,
            reason,
        }
    }

    pub const fn warn(reason: &'static str) -> Self {
        Self {
            level: ArrowCloudAutosubmitLogLevel::Warn,
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudAutosubmitSessionDecision {
    Submit,
    Skip {
        log: Option<ArrowCloudAutosubmitLog>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrowCloudAutosubmitSessionInput {
    pub enabled: bool,
    pub player_count: usize,
    pub autoplay_used: bool,
    pub is_course_stage: bool,
    pub autosubmit_course_scores_individually: bool,
}

pub const fn arrowcloud_autosubmit_session_decision(
    input: ArrowCloudAutosubmitSessionInput,
) -> ArrowCloudAutosubmitSessionDecision {
    if !input.enabled || input.player_count == 0 {
        return ArrowCloudAutosubmitSessionDecision::Skip { log: None };
    }
    if input.autoplay_used {
        return ArrowCloudAutosubmitSessionDecision::Skip {
            log: Some(ArrowCloudAutosubmitLog::debug("autoplay/replay was used")),
        };
    }
    if input.is_course_stage && !input.autosubmit_course_scores_individually {
        return ArrowCloudAutosubmitSessionDecision::Skip {
            log: Some(ArrowCloudAutosubmitLog::debug(
                "course per-song autosubmit is disabled",
            )),
        };
    }
    ArrowCloudAutosubmitSessionDecision::Submit
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudAutosubmitPlayerAction {
    BuildPayload,
    Skip {
        log: Option<ArrowCloudAutosubmitLog>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowCloudAutosubmitPlayerDecision {
    pub failed: bool,
    pub passed: bool,
    pub allow_failed_submit: bool,
    pub action: ArrowCloudAutosubmitPlayerAction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArrowCloudAutosubmitPlayerInput {
    pub song_has_lua: bool,
    pub lua_submit_allowed: bool,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub has_fail_time: bool,
    pub submit_fails_enabled: bool,
    pub api_key_present: bool,
    pub course_stage_life_submit_eligible: bool,
}

pub const fn arrowcloud_autosubmit_player_decision(
    input: ArrowCloudAutosubmitPlayerInput,
) -> ArrowCloudAutosubmitPlayerDecision {
    if input.song_has_lua && !input.lua_submit_allowed {
        return ArrowCloudAutosubmitPlayerDecision {
            failed: false,
            passed: false,
            allow_failed_submit: false,
            action: ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::debug("simfile relies on lua")),
            },
        };
    }

    let failed = gameplay_run_failed(input.is_failing, input.has_fail_time);
    let passed = gameplay_run_passed(
        input.song_completed_naturally,
        input.is_failing,
        input.life,
        input.has_fail_time,
    );
    let allow_failed_submit = failed && input.submit_fails_enabled;
    if !input.song_completed_naturally && !failed {
        return ArrowCloudAutosubmitPlayerDecision {
            failed,
            passed,
            allow_failed_submit,
            action: ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::debug("stage was not completed")),
            },
        };
    }
    if !input.api_key_present {
        let log = if passed || allow_failed_submit {
            Some(ArrowCloudAutosubmitLog::warn("profile is missing API key"))
        } else {
            None
        };
        return ArrowCloudAutosubmitPlayerDecision {
            failed,
            passed,
            allow_failed_submit,
            action: ArrowCloudAutosubmitPlayerAction::Skip { log },
        };
    }
    if !input.course_stage_life_submit_eligible && !allow_failed_submit {
        return ArrowCloudAutosubmitPlayerDecision {
            failed,
            passed,
            allow_failed_submit,
            action: ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::warn(
                    "course stage would have failed from normal life",
                )),
            },
        };
    }
    ArrowCloudAutosubmitPlayerDecision {
        failed,
        passed,
        allow_failed_submit,
        action: ArrowCloudAutosubmitPlayerAction::BuildPayload,
    }
}

pub const fn arrowcloud_autosubmit_after_payload_decision(
    failed: bool,
    allow_failed_submit: bool,
) -> Option<ArrowCloudAutosubmitLog> {
    if failed && !allow_failed_submit {
        Some(ArrowCloudAutosubmitLog::debug(
            "failed-stage submits are disabled",
        ))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsAutosubmitLogLevel {
    Debug,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrooveStatsAutosubmitLog {
    pub level: GrooveStatsAutosubmitLogLevel,
    pub reason: &'static str,
}

impl GrooveStatsAutosubmitLog {
    pub const fn debug(reason: &'static str) -> Self {
        Self {
            level: GrooveStatsAutosubmitLogLevel::Debug,
            reason,
        }
    }

    pub const fn warn(reason: &'static str) -> Self {
        Self {
            level: GrooveStatsAutosubmitLogLevel::Warn,
            reason,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsAutosubmitSessionDecision {
    Submit,
    Skip {
        log: Option<GrooveStatsAutosubmitLog>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrooveStatsAutosubmitSessionInput {
    pub enabled: bool,
    pub player_count: usize,
    pub autoplay_used: bool,
    pub is_course_stage: bool,
    pub autosubmit_course_scores_individually: bool,
}

pub const fn groovestats_autosubmit_session_decision(
    input: GrooveStatsAutosubmitSessionInput,
) -> GrooveStatsAutosubmitSessionDecision {
    if !input.enabled || input.player_count == 0 {
        return GrooveStatsAutosubmitSessionDecision::Skip { log: None };
    }
    if input.autoplay_used {
        return GrooveStatsAutosubmitSessionDecision::Skip {
            log: Some(GrooveStatsAutosubmitLog::debug("autoplay/replay was used")),
        };
    }
    if input.is_course_stage && !input.autosubmit_course_scores_individually {
        return GrooveStatsAutosubmitSessionDecision::Skip {
            log: Some(GrooveStatsAutosubmitLog::debug(
                "course per-song autosubmit is disabled",
            )),
        };
    }
    GrooveStatsAutosubmitSessionDecision::Submit
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsAutosubmitPlayerAction {
    BuildPayload,
    SkipInvalidReason,
    Skip {
        log: Option<GrooveStatsAutosubmitLog>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrooveStatsAutosubmitPlayerDecision {
    pub failed: bool,
    pub passed: bool,
    pub action: GrooveStatsAutosubmitPlayerAction,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrooveStatsAutosubmitPlayerInput {
    pub has_invalid_reason: bool,
    pub is_pad_player: bool,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub has_fail_time: bool,
    pub course_stage_life_submit_eligible: bool,
    pub api_key_present: bool,
}

pub const fn groovestats_autosubmit_player_decision(
    input: GrooveStatsAutosubmitPlayerInput,
) -> GrooveStatsAutosubmitPlayerDecision {
    let failed = gameplay_run_failed(input.is_failing, input.has_fail_time);
    let passed = gameplay_run_passed(
        input.song_completed_naturally,
        input.is_failing,
        input.life,
        input.has_fail_time,
    );
    if input.has_invalid_reason {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::SkipInvalidReason,
        };
    }
    if !input.is_pad_player {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::warn(
                    "profile is not marked as a pad player",
                )),
            },
        };
    }
    if !input.song_completed_naturally && !failed {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug("stage was not completed")),
            },
        };
    }
    if !passed {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug("stage was not passed")),
            },
        };
    }
    if !input.course_stage_life_submit_eligible {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::warn(
                    "course stage would have failed from normal life",
                )),
            },
        };
    }
    if !input.api_key_present {
        return GrooveStatsAutosubmitPlayerDecision {
            failed,
            passed,
            action: GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::warn("profile is missing API key")),
            },
        };
    }
    GrooveStatsAutosubmitPlayerDecision {
        failed,
        passed,
        action: GrooveStatsAutosubmitPlayerAction::BuildPayload,
    }
}

#[inline(always)]
pub fn promote_quint_grade(grade: Grade, ex_score_percent: f64) -> Grade {
    if grade != Grade::Failed && ex_score_percent >= 100.0 {
        Grade::Quint
    } else {
        grade
    }
}

pub fn score_to_grade(score: f64) -> Grade {
    let percent = score / 10000.0;
    if percent >= 1.00 {
        Grade::Tier01
    } else if percent >= 0.99 {
        Grade::Tier02
    } else if percent >= 0.98 {
        Grade::Tier03
    } else if percent >= 0.96 {
        Grade::Tier04
    } else if percent >= 0.94 {
        Grade::Tier05
    } else if percent >= 0.92 {
        Grade::Tier06
    } else if percent >= 0.89 {
        Grade::Tier07
    } else if percent >= 0.86 {
        Grade::Tier08
    } else if percent >= 0.83 {
        Grade::Tier09
    } else if percent >= 0.80 {
        Grade::Tier10
    } else if percent >= 0.76 {
        Grade::Tier11
    } else if percent >= 0.72 {
        Grade::Tier12
    } else if percent >= 0.68 {
        Grade::Tier13
    } else if percent >= 0.64 {
        Grade::Tier14
    } else if percent >= 0.60 {
        Grade::Tier15
    } else if percent >= 0.55 {
        Grade::Tier16
    } else {
        Grade::Tier17
    }
}

pub const fn grade_to_code(grade: Grade) -> u8 {
    match grade {
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

pub const fn grade_from_code(code: u8) -> Grade {
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

#[inline(always)]
pub const fn cached_score(
    grade: Grade,
    score_percent: f64,
    lamp_index: Option<u8>,
    lamp_judge_count: Option<u8>,
) -> CachedScore {
    if matches!(grade, Grade::Failed) {
        CachedScore {
            grade,
            score_percent,
            lamp_index: None,
            lamp_judge_count: None,
        }
    } else {
        CachedScore {
            grade,
            score_percent,
            lamp_index,
            lamp_judge_count,
        }
    }
}

#[inline(always)]
pub fn cached_score_10000(score: &CachedScore) -> f64 {
    score.score_percent * 10000.0
}

#[inline(always)]
pub fn same_score_10000(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a.round() - b.round()).abs() <= 1.0
}

#[inline(always)]
const fn grade_priority(grade: Grade) -> u8 {
    match grade {
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
        Grade::Failed => u8::MAX,
    }
}

#[inline(always)]
const fn lamp_priority(lamp_index: Option<u8>) -> u8 {
    match lamp_index {
        Some(idx @ 0..=4) => idx,
        Some(_) | None => u8::MAX,
    }
}

#[inline(always)]
const fn lamp_judge_count_priority(count: Option<u8>) -> u8 {
    match count {
        Some(value) => value,
        None => u8::MAX,
    }
}

#[inline(always)]
pub fn is_better_itg(new: &CachedScore, old: &CachedScore) -> bool {
    match (old.grade == Grade::Failed, new.grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    if !same_score_10000(cached_score_10000(new), cached_score_10000(old)) {
        return new.score_percent > old.score_percent;
    }

    let new_grade = grade_priority(new.grade);
    let old_grade = grade_priority(old.grade);
    if new_grade != old_grade {
        return new_grade < old_grade;
    }

    let new_lamp = lamp_priority(new.lamp_index);
    let old_lamp = lamp_priority(old.lamp_index);
    if new_lamp != old_lamp {
        return new_lamp < old_lamp;
    }

    let new_count = lamp_judge_count_priority(new.lamp_judge_count);
    let old_count = lamp_judge_count_priority(old.lamp_judge_count);
    new_count < old_count
}

#[inline(always)]
pub fn is_better_scalar_score(
    new_grade: Grade,
    new_percent: f64,
    old_grade: Grade,
    old_percent: f64,
) -> bool {
    match (old_grade == Grade::Failed, new_grade == Grade::Failed) {
        (true, false) => return true,
        (false, true) => return false,
        _ => {}
    }
    new_percent > old_percent
}

#[inline(always)]
pub fn failed_score_override(a: &CachedScore, b: &CachedScore) -> Option<CachedScore> {
    if a.grade == Grade::Failed
        && b.grade != Grade::Failed
        && same_score_10000(cached_score_10000(a), cached_score_10000(b))
    {
        Some(*a)
    } else if b.grade == Grade::Failed
        && a.grade != Grade::Failed
        && same_score_10000(cached_score_10000(a), cached_score_10000(b))
    {
        Some(*b)
    } else {
        None
    }
}

#[inline(always)]
pub fn replaces_stale_quint(
    score: &CachedScore,
    existing: &CachedScore,
    proves_nonquint_ex: bool,
) -> bool {
    proves_nonquint_ex
        && existing.grade == Grade::Quint
        && existing.lamp_index == Some(0)
        && score.grade == Grade::Tier01
        && score.lamp_index == Some(1)
        && same_score_10000(cached_score_10000(score), cached_score_10000(existing))
}

#[inline(always)]
pub fn should_replace_cached_gs_score(
    score: &CachedScore,
    existing: &CachedScore,
    proves_nonquint_ex: bool,
) -> bool {
    replaces_stale_quint(score, existing, proves_nonquint_ex)
        || failed_score_override(score, existing) == Some(*score)
        || is_better_itg(score, existing)
}

#[inline(always)]
const fn non_quint_grade(grade: Grade) -> Grade {
    match grade {
        Grade::Quint => Grade::Tier01,
        _ => grade,
    }
}

#[inline(always)]
fn fix_quint_grade_lamp(grade: Grade, score_percent: f64, lamp_index: Option<u8>) -> Grade {
    if grade == Grade::Failed {
        return Grade::Failed;
    }
    let grade = if grade == Grade::Quint {
        score_to_grade(score_percent.clamp(0.0, 1.0) * 10000.0)
    } else {
        grade
    };
    if lamp_index == Some(0) {
        Grade::Quint
    } else {
        grade
    }
}

#[inline(always)]
pub fn fix_gs_cached_score(score: CachedScore) -> CachedScore {
    cached_score(
        fix_quint_grade_lamp(score.grade, score.score_percent, score.lamp_index),
        score.score_percent,
        score.lamp_index,
        score.lamp_judge_count,
    )
}

#[inline(always)]
pub fn fix_local_ex_grade(grade: Grade, ex_score_percent: f64) -> Grade {
    if grade == Grade::Failed {
        Grade::Failed
    } else {
        promote_quint_grade(non_quint_grade(grade), ex_score_percent)
    }
}

pub const LOCAL_SCORE_VERSION: u16 = 1;
pub const LOCAL_SCORE_INDEX_VERSION: u16 = 3;

#[derive(Debug, Clone, Encode, Decode)]
struct LocalScoreIndexFile {
    version: u16,
    index: LocalScoreIndex,
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct LocalReplayEdge {
    pub event_music_time_ns: SongTimeNs,
    pub lane: u8,
    pub pressed: bool,
    // 0 = Keyboard, 1 = Gamepad
    pub source: u8,
}

impl LocalReplayEdge {
    #[inline(always)]
    pub const fn new(
        event_music_time_ns: SongTimeNs,
        lane: u8,
        pressed: bool,
        source: InputSource,
    ) -> Self {
        Self {
            event_music_time_ns,
            lane,
            pressed,
            source: Self::source_code(source),
        }
    }

    #[inline(always)]
    pub const fn input_source(self) -> InputSource {
        match self.source {
            1 => InputSource::Gamepad,
            _ => InputSource::Keyboard,
        }
    }

    #[inline(always)]
    pub const fn source_code(source: InputSource) -> u8 {
        match source {
            InputSource::Keyboard => 0,
            InputSource::Gamepad => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalReplayEdgeInput {
    pub event_music_time_ns: SongTimeNs,
    pub lane_index: u8,
    pub pressed: bool,
    pub source: InputSource,
}

pub fn local_replay_edges_for_player(
    replay_edges: impl IntoIterator<Item = LocalReplayEdgeInput>,
    player: usize,
    num_players: usize,
    num_cols: usize,
    cols_per_player: usize,
) -> Vec<LocalReplayEdge> {
    let replay_edges = replay_edges.into_iter();
    let (col_start, col_end) = if num_players <= 1 {
        (0usize, num_cols)
    } else {
        let start = player.saturating_mul(cols_per_player);
        (start, start.saturating_add(cols_per_player))
    };

    let mut out = Vec::new();
    out.reserve(replay_edges.size_hint().1.unwrap_or(4096).min(4096));
    for e in replay_edges {
        let lane = e.lane_index as usize;
        if lane < col_start || lane >= col_end || song_time_ns_invalid(e.event_music_time_ns) {
            continue;
        }
        out.push(LocalReplayEdge::new(
            e.event_music_time_ns,
            (lane - col_start) as u8,
            e.pressed,
            e.source,
        ));
    }
    out
}

pub struct LocalScoreGameplaySaveInput<'a> {
    pub autoplay_used: bool,
    pub score_valid: bool,
    pub invalid_reasons: &'a [&'a str],
    pub scoring_counts: &'a judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub possible_grade_points: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LocalScoreGameplaySaveDecision {
    SkipAutoplay,
    SkipInvalid { detail: String },
    Save { score_percent: f64 },
}

pub struct LocalScoreGameplayPlayer<'a> {
    pub player_idx: usize,
    pub profile_id: String,
    pub profile_initials: &'a str,
    pub chart_hash: &'a str,
    pub invalid_reasons: Vec<&'a str>,
    pub score_valid: bool,
    pub scoring_counts: &'a judgment::JudgeCounts,
    pub holds_held_for_score: u32,
    pub rolls_held_for_score: u32,
    pub mines_hit_for_score: u32,
    pub possible_grade_points: i32,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub fail_time: Option<f32>,
    pub notes: &'a [Note],
    pub note_times: &'a [SongTimeNs],
    pub hold_end_times: &'a [Option<SongTimeNs>],
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub counts: [u32; 6],
    pub white_fantastics: Option<u32>,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub hands_achieved: u32,
    pub beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalScoreGameplaySaveSkip {
    Autoplay,
    Invalid { player_idx: usize, detail: String },
}

pub fn local_score_gameplay_save_decision(
    input: LocalScoreGameplaySaveInput<'_>,
) -> LocalScoreGameplaySaveDecision {
    if input.autoplay_used {
        return LocalScoreGameplaySaveDecision::SkipAutoplay;
    }
    if !input.score_valid {
        let detail = if input.invalid_reasons.is_empty() {
            "ranking-invalid modifiers were used".to_string()
        } else {
            input.invalid_reasons.join("; ")
        };
        return LocalScoreGameplaySaveDecision::SkipInvalid { detail };
    }
    LocalScoreGameplaySaveDecision::Save {
        score_percent: judgment::calculate_itg_score_percent_from_counts(
            input.scoring_counts,
            input.holds_held_for_score,
            input.rolls_held_for_score,
            input.mines_hit_for_score,
            input.possible_grade_points,
        ),
    }
}

pub fn save_local_gameplay_scores<'a, W, L>(
    played_at_ms: i64,
    music_rate: f32,
    autoplay_used: bool,
    players: impl IntoIterator<Item = LocalScoreGameplayPlayer<'a>>,
    mut write_score: W,
    mut log_skip: L,
) where
    W: FnMut(&str, &str, &str, &mut LocalScoreEntry) -> bool,
    L: FnMut(LocalScoreGameplaySaveSkip),
{
    for player in players {
        let save_decision = local_score_gameplay_save_decision(LocalScoreGameplaySaveInput {
            autoplay_used,
            score_valid: player.score_valid,
            invalid_reasons: player.invalid_reasons.as_slice(),
            scoring_counts: player.scoring_counts,
            holds_held_for_score: player.holds_held_for_score,
            rolls_held_for_score: player.rolls_held_for_score,
            mines_hit_for_score: player.mines_hit_for_score,
            possible_grade_points: player.possible_grade_points,
        });
        let score_percent = match save_decision {
            LocalScoreGameplaySaveDecision::SkipAutoplay => {
                log_skip(LocalScoreGameplaySaveSkip::Autoplay);
                return;
            }
            LocalScoreGameplaySaveDecision::SkipInvalid { detail } => {
                log_skip(LocalScoreGameplaySaveSkip::Invalid {
                    player_idx: player.player_idx,
                    detail,
                });
                continue;
            }
            LocalScoreGameplaySaveDecision::Save { score_percent } => score_percent,
        };

        let mut entry = local_score_entry_from_gameplay_input(LocalScoreGameplayEntryInput {
            played_at_ms,
            music_rate,
            score_percent,
            song_completed_naturally: player.song_completed_naturally,
            is_failing: player.is_failing,
            life: player.life,
            fail_time: player.fail_time,
            notes: player.notes,
            note_times: player.note_times,
            hold_end_times: player.hold_end_times,
            total_steps: player.total_steps,
            holds_total: player.holds_total,
            rolls_total: player.rolls_total,
            mines_total: player.mines_total,
            counts: player.counts,
            white_fantastics: player.white_fantastics,
            holds_held: player.holds_held,
            rolls_held: player.rolls_held,
            mines_avoided: player.mines_avoided,
            hands_achieved: player.hands_achieved,
            beat0_time_ns: player.beat0_time_ns,
            replay: player.replay,
        });
        write_score(
            player.profile_id.as_str(),
            player.profile_initials,
            player.chart_hash,
            &mut entry,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSummaryScoreSaveDecision {
    SkipEmptyChartHash,
    SkipDisqualified,
    SkipInvalid,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalSummaryScoreSaveInput<'a> {
    pub chart_hash: &'a str,
    pub disqualified: bool,
    pub score_valid: bool,
}

pub fn local_summary_score_save_decision(
    input: LocalSummaryScoreSaveInput<'_>,
) -> LocalSummaryScoreSaveDecision {
    if input.chart_hash.trim().is_empty() {
        return LocalSummaryScoreSaveDecision::SkipEmptyChartHash;
    }
    if input.disqualified {
        return LocalSummaryScoreSaveDecision::SkipDisqualified;
    }
    if !input.score_valid {
        return LocalSummaryScoreSaveDecision::SkipInvalid;
    }
    LocalSummaryScoreSaveDecision::Save
}

#[derive(Debug, Clone, Copy, PartialEq, Encode, Decode)]
pub struct LocalScoreHeader {
    pub version: u16,
    pub played_at_ms: i64,
    pub music_rate: f32,
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    // Fantastic, Excellent, Great, Decent, WayOff, Miss (row judgments)
    pub judgment_counts: [u32; 6],
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub hands_achieved: u32,
    pub fail_time: Option<f32>,
    pub beat0_time_ns: SongTimeNs,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LocalScoreEntry {
    pub version: u16,
    pub played_at_ms: i64,
    pub music_rate: f32,
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub ex_score_percent: f64,
    pub hard_ex_score_percent: f64,
    pub judgment_counts: [u32; 6],
    pub holds_held: u32,
    pub holds_total: u32,
    pub rolls_held: u32,
    pub rolls_total: u32,
    pub mines_avoided: u32,
    pub mines_total: u32,
    pub hands_achieved: u32,
    pub fail_time: Option<f32>,
    pub beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

impl LocalScoreEntry {
    pub fn header(&self) -> LocalScoreHeader {
        LocalScoreHeader {
            version: self.version,
            played_at_ms: self.played_at_ms,
            music_rate: self.music_rate,
            score_percent: self.score_percent,
            grade_code: self.grade_code,
            lamp_index: self.lamp_index,
            lamp_judge_count: self.lamp_judge_count,
            ex_score_percent: self.ex_score_percent,
            hard_ex_score_percent: self.hard_ex_score_percent,
            judgment_counts: self.judgment_counts,
            holds_held: self.holds_held,
            holds_total: self.holds_total,
            rolls_held: self.rolls_held,
            rolls_total: self.rolls_total,
            mines_avoided: self.mines_avoided,
            mines_total: self.mines_total,
            hands_achieved: self.hands_achieved,
            fail_time: self.fail_time,
            beat0_time_ns: self.beat0_time_ns,
        }
    }
}

pub struct LocalScoreGameplayEntryInput<'a> {
    pub played_at_ms: i64,
    pub music_rate: f32,
    pub score_percent: f64,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub fail_time: Option<f32>,
    pub notes: &'a [Note],
    pub note_times: &'a [SongTimeNs],
    pub hold_end_times: &'a [Option<SongTimeNs>],
    pub total_steps: u32,
    pub holds_total: u32,
    pub rolls_total: u32,
    pub mines_total: u32,
    pub counts: [u32; 6],
    pub white_fantastics: Option<u32>,
    pub holds_held: u32,
    pub rolls_held: u32,
    pub mines_avoided: u32,
    pub hands_achieved: u32,
    pub beat0_time_ns: SongTimeNs,
    pub replay: Vec<LocalReplayEdge>,
}

pub fn local_score_entry_from_gameplay_input(
    input: LocalScoreGameplayEntryInput<'_>,
) -> LocalScoreEntry {
    let mines_disabled = false;
    let mut grade = if gameplay_run_passed(
        input.song_completed_naturally,
        input.is_failing,
        input.life,
        input.fail_time.is_some(),
    ) {
        score_to_grade(input.score_percent * 10000.0)
    } else {
        Grade::Failed
    };

    let ex_score_percent = judgment::calculate_ex_score_from_notes(
        input.notes,
        input.note_times,
        input.hold_end_times,
        input.total_steps,
        input.holds_total,
        input.rolls_total,
        input.mines_total,
        input.fail_time.map(song_time_ns_from_seconds),
        mines_disabled,
    );
    let hard_ex_score_percent = judgment::calculate_hard_ex_score_from_notes(
        input.notes,
        input.note_times,
        input.hold_end_times,
        input.total_steps,
        input.holds_total,
        input.rolls_total,
        input.mines_total,
        input.fail_time.map(song_time_ns_from_seconds),
        mines_disabled,
    );

    grade = promote_quint_grade(grade, ex_score_percent);
    let (lamp_index, lamp_judge_count) =
        compute_local_lamp(input.counts, grade, input.white_fantastics);

    LocalScoreEntry {
        version: LOCAL_SCORE_VERSION,
        played_at_ms: input.played_at_ms,
        music_rate: input.music_rate,
        score_percent: input.score_percent,
        grade_code: grade_to_code(grade),
        lamp_index,
        lamp_judge_count,
        ex_score_percent,
        hard_ex_score_percent,
        judgment_counts: input.counts,
        holds_held: input.holds_held,
        holds_total: input.holds_total,
        rolls_held: input.rolls_held,
        rolls_total: input.rolls_total,
        mines_avoided: input.mines_avoided,
        mines_total: input.mines_total,
        hands_achieved: input.hands_achieved,
        fail_time: input.fail_time,
        beat0_time_ns: input.beat0_time_ns,
        replay: input.replay,
    }
}

pub fn local_score_entry_from_stage_summary(
    played_at_ms: i64,
    music_rate: f32,
    summary: &stage_stats::PlayerStageSummary,
) -> LocalScoreEntry {
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
    let (lamp_index, lamp_judge_count) =
        compute_local_lamp(counts, summary.grade, Some(summary.window_counts.w1));
    LocalScoreEntry {
        version: LOCAL_SCORE_VERSION,
        played_at_ms,
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
        beat0_time_ns: 0,
        replay: Vec::new(),
    }
}

#[inline(always)]
fn local_lamp_judge_count(count: u32) -> Option<u8> {
    if (1..=9).contains(&count) {
        Some(count as u8)
    } else {
        None
    }
}

pub fn compute_local_lamp(
    counts: [u32; 6],
    grade: Grade,
    white_fantastics: Option<u32>,
) -> (Option<u8>, Option<u8>) {
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
        return (Some(1), white_fantastics.and_then(local_lamp_judge_count));
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

#[inline(always)]
const fn local_score_grade(grade_code: u8, has_fail_time: bool) -> Grade {
    if has_fail_time {
        Grade::Failed
    } else {
        grade_from_code(grade_code)
    }
}

#[inline(always)]
pub fn cached_score_from_local_header(h: &LocalScoreHeader) -> CachedScore {
    let grade = fix_local_ex_grade(
        local_score_grade(h.grade_code, h.fail_time.is_some()),
        h.ex_score_percent,
    );
    let (lamp_index, lamp_judge_count) = if grade == Grade::Quint {
        (Some(0), None)
    } else if h.lamp_index == Some(0) {
        compute_local_lamp(h.judgment_counts, grade, None)
    } else {
        (h.lamp_index, h.lamp_judge_count)
    };
    cached_score(grade, h.score_percent, lamp_index, lamp_judge_count)
}

pub fn decode_local_score_header(bytes: &[u8]) -> Option<LocalScoreHeader> {
    let Ok((h, _)) =
        bincode::decode_from_slice::<LocalScoreHeader, _>(bytes, bincode::config::standard())
    else {
        return None;
    };
    if h.version != LOCAL_SCORE_VERSION {
        return None;
    }
    Some(h)
}

pub fn decode_local_score_entry(bytes: &[u8]) -> Option<LocalScoreEntry> {
    let Ok((entry, _)) =
        bincode::decode_from_slice::<LocalScoreEntry, _>(bytes, bincode::config::standard())
    else {
        return None;
    };
    if entry.version != LOCAL_SCORE_VERSION {
        return None;
    }
    Some(entry)
}

pub fn encode_local_score_entry(entry: &LocalScoreEntry) -> Option<Vec<u8>> {
    bincode::encode_to_vec(entry, bincode::config::standard()).ok()
}

pub fn decode_local_score_index(bytes: &[u8]) -> Option<LocalScoreIndex> {
    let (file, _) =
        bincode::decode_from_slice::<LocalScoreIndexFile, _>(bytes, bincode::config::standard())
            .ok()?;
    (file.version == LOCAL_SCORE_INDEX_VERSION).then_some(file.index)
}

pub fn encode_local_score_index(index: &LocalScoreIndex) -> Option<Vec<u8>> {
    bincode::encode_to_vec(
        LocalScoreIndexFile {
            version: LOCAL_SCORE_INDEX_VERSION,
            index: index.clone(),
        },
        bincode::config::standard(),
    )
    .ok()
}

fn update_local_score_best_scalar(
    best_by_chart: &mut HashMap<String, LocalScoreBestScalar>,
    chart_hash: &str,
    new: LocalScoreBestScalar,
) {
    match best_by_chart.get_mut(chart_hash) {
        Some(existing) => {
            if is_better_scalar_score(new.grade, new.percent, existing.grade, existing.percent) {
                *existing = new;
            }
        }
        None => {
            best_by_chart.insert(chart_hash.to_string(), new);
        }
    }
}

pub fn update_local_score_index(
    index: &mut LocalScoreIndex,
    chart_hash: &str,
    header: &LocalScoreHeader,
) {
    let cached = cached_score_from_local_header(header);
    let grade = cached.grade;

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

    if grade != Grade::Failed {
        let rate = groovestats_rate_hundredths(header.music_rate);
        if rate >= 100 {
            index
                .best_pass_rate
                .entry(chart_hash.to_string())
                .and_modify(|best| *best = (*best).max(rate))
                .or_insert(rate);
        }
    }

    update_local_score_best_scalar(
        &mut index.best_ex,
        chart_hash,
        LocalScoreBestScalar {
            grade,
            percent: header.ex_score_percent,
        },
    );
    update_local_score_best_scalar(
        &mut index.best_hard_ex,
        chart_hash,
        LocalScoreBestScalar {
            grade,
            percent: header.hard_ex_score_percent,
        },
    );
}

#[derive(Debug, Clone, Encode, Decode)]
struct GsScoreEntryV1 {
    score_percent: f64,
    grade_code: u8,
    lamp_index: Option<u8>,
    username: String,
    fetched_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct GsScoreEntry {
    pub score_percent: f64,
    pub grade_code: u8,
    pub lamp_index: Option<u8>,
    pub lamp_judge_count: Option<u8>,
    pub username: String,
    pub fetched_at_ms: i64,
}

pub fn gs_score_entry_from_cached(
    score: CachedScore,
    username: &str,
    fetched_at_ms: i64,
) -> GsScoreEntry {
    let score = fix_gs_cached_score(score);
    GsScoreEntry {
        score_percent: score.score_percent,
        grade_code: grade_to_code(score.grade),
        lamp_index: score.lamp_index,
        lamp_judge_count: score.lamp_judge_count,
        username: username.to_string(),
        fetched_at_ms,
    }
}

pub fn cached_score_from_gs_entry(entry: &GsScoreEntry) -> CachedScore {
    fix_gs_cached_score(cached_score(
        grade_from_code(entry.grade_code),
        entry.score_percent,
        entry.lamp_index,
        entry.lamp_judge_count,
    ))
}

pub fn decode_gs_score_entry(bytes: &[u8]) -> Option<GsScoreEntry> {
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

pub fn encode_gs_score_entry(entry: &GsScoreEntry) -> Option<Vec<u8>> {
    bincode::encode_to_vec(entry, bincode::config::standard()).ok()
}

#[inline(always)]
pub fn leaderboard_rank_for_score(entries: &[LeaderboardEntry], score_percent: f64) -> Option<u32> {
    if !score_percent.is_finite() {
        return None;
    }
    let target = (score_percent * 10000.0).round();
    let higher_scores = entries
        .iter()
        .filter(|entry| entry.score - target > 0.5)
        .count();
    Some((higher_scores as u32).saturating_add(1))
}

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreImportCredentialError {
    MissingApiKey { endpoint: ScoreImportEndpoint },
    MissingUsername { endpoint: ScoreImportEndpoint },
}

impl ScoreImportCredentialError {
    pub fn request_message(&self) -> String {
        match self {
            Self::MissingApiKey { endpoint } => {
                format!(
                    "{} API key is missing in profile configuration.",
                    endpoint.display_name()
                )
            }
            Self::MissingUsername { endpoint } => {
                format!(
                    "{} username is missing in profile configuration.",
                    endpoint.display_name()
                )
            }
        }
    }

    pub fn import_message(&self) -> String {
        match self {
            Self::MissingApiKey { endpoint } => {
                format!(
                    "{} API key is not set in profile configuration.",
                    endpoint.display_name()
                )
            }
            Self::MissingUsername { endpoint } => {
                format!(
                    "{} username is not set in profile configuration.",
                    endpoint.display_name()
                )
            }
        }
    }
}

pub fn validate_score_import_credentials(
    endpoint: ScoreImportEndpoint,
    api_key: &str,
    username: &str,
) -> Result<(), ScoreImportCredentialError> {
    if api_key.trim().is_empty() {
        return Err(ScoreImportCredentialError::MissingApiKey { endpoint });
    }
    if endpoint.requires_username() && username.trim().is_empty() {
        return Err(ScoreImportCredentialError::MissingUsername { endpoint });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct ScoreFetchAndCacheInput<'a> {
    pub credentials_ready: bool,
    pub missing_credentials_message: &'a str,
    pub username: &'a str,
    pub chart_hash: &'a str,
}

pub fn fetch_and_cache_score<F, C, M>(
    input: ScoreFetchAndCacheInput<'_>,
    fetch_score: F,
    cache_score: C,
    cache_missing_score: M,
) -> Result<(), Box<dyn Error + Send + Sync>>
where
    F: FnOnce(&str) -> Result<CachedScoreImportResult, Box<dyn Error + Send + Sync>>,
    C: FnOnce(CachedScore, bool),
    M: FnOnce(CachedScore),
{
    if !input.credentials_ready {
        return Err(input.missing_credentials_message.into());
    }

    debug!(
        "Requesting scores for '{}' on chart '{}'...",
        input.username, input.chart_hash
    );

    let result = fetch_score(input.chart_hash)?;
    if let Some(cached_score) = result.score {
        cache_score(cached_score, result.score_proves_nonquint_ex);
    } else {
        warn!(
            "No score found for player '{}' on chart '{}'. Caching as Failed.",
            input.username, input.chart_hash
        );
        cache_missing_score(cached_missing_gs_score());
    }

    Ok(())
}

pub const SCORE_IMPORT_ENDPOINT_CHOICES: [ScoreImportEndpoint; 3] = [
    ScoreImportEndpoint::GrooveStats,
    ScoreImportEndpoint::BoogieStats,
    ScoreImportEndpoint::ArrowCloud,
];

#[inline(always)]
pub const fn score_import_endpoint_choice_index(endpoint: ScoreImportEndpoint) -> usize {
    match endpoint {
        ScoreImportEndpoint::GrooveStats => 0,
        ScoreImportEndpoint::BoogieStats => 1,
        ScoreImportEndpoint::ArrowCloud => 2,
    }
}

#[inline(always)]
pub const fn score_import_endpoint_from_choice_index(idx: usize) -> ScoreImportEndpoint {
    match idx {
        1 => ScoreImportEndpoint::BoogieStats,
        2 => ScoreImportEndpoint::ArrowCloud,
        _ => ScoreImportEndpoint::GrooveStats,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedPlayerScore {
    pub score_10000: f64,
    pub comments: Option<String>,
    pub is_fail: bool,
    pub ex_evidence: GsExEvidence,
}

impl ImportedPlayerScore {
    #[inline(always)]
    pub fn needs_chart_stats(&self) -> bool {
        let score_percent = self.score_10000 / 10000.0;
        (score_percent - 1.0).abs() > 1e-9 && self.comments.is_some()
    }
}

pub fn merge_local_fail(
    mut imported: ImportedPlayerScore,
    local_score: Option<CachedScore>,
) -> ImportedPlayerScore {
    if local_score.is_some_and(|score| {
        score.grade == Grade::Failed
            && same_score_10000(cached_score_10000(&score), imported.score_10000)
    }) {
        imported.is_fail = true;
    }
    imported
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerScoreImportResult {
    pub score: Option<ImportedPlayerScore>,
    pub score_proves_nonquint_ex: bool,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
    pub itl_self_found: bool,
}

impl PlayerScoreImportResult {
    #[inline(always)]
    pub const fn empty() -> Self {
        Self {
            score: None,
            score_proves_nonquint_ex: false,
            itl_self_score: None,
            itl_self_rank: None,
            itl_self_found: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CachedScoreImportResult {
    pub score: Option<CachedScore>,
    pub score_proves_nonquint_ex: bool,
    pub itl_self_score: Option<u32>,
    pub itl_self_rank: Option<u32>,
    pub itl_self_found: bool,
}

pub fn cached_score_from_imported_player_score(
    score: ImportedPlayerScore,
    stats: Option<GsLampChartStats>,
) -> CachedScore {
    let ImportedPlayerScore {
        score_10000,
        comments,
        is_fail,
        ex_evidence,
    } = score;
    let stats = if (score_10000 / 10000.0 - 1.0).abs() > 1e-9 && comments.is_some() {
        stats
    } else {
        None
    };
    cached_gs_score_from_chart_stats(
        score_10000,
        is_fail,
        comments.as_deref(),
        ex_evidence,
        stats,
    )
}

pub fn cached_score_import_result_from_imported(
    imported: PlayerScoreImportResult,
    stats: Option<GsLampChartStats>,
) -> CachedScoreImportResult {
    CachedScoreImportResult {
        score: imported
            .score
            .map(|score| cached_score_from_imported_player_score(score, stats)),
        score_proves_nonquint_ex: imported.score_proves_nonquint_ex,
        itl_self_score: imported.itl_self_score,
        itl_self_rank: imported.itl_self_rank,
        itl_self_found: imported.itl_self_found,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeaderboardCachedScore {
    pub score: CachedScore,
    pub score_proves_nonquint_ex: bool,
}

pub fn cached_score_from_leaderboard_import(
    imported: Option<ImportedPlayerScore>,
    local_score: Option<CachedScore>,
    stats: Option<GsLampChartStats>,
) -> Option<LeaderboardCachedScore> {
    let imported = merge_local_fail(imported?, local_score);
    let score_proves_nonquint_ex = imported.ex_evidence.proves_nonquint();
    let score = cached_score_from_imported_player_score(imported, stats);
    Some(LeaderboardCachedScore {
        score,
        score_proves_nonquint_ex,
    })
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

pub const SCORE_IMPORT_RATE_LIMIT_PER_SECOND: u32 = 3;
pub const SCORE_IMPORT_REQUEST_INTERVAL: Duration = Duration::from_millis(334);
pub const SCORE_IMPORT_PROGRESS_LOG_EVERY: usize = 100;

#[inline(always)]
pub const fn score_import_filter_note(only_missing_scores: bool) -> &'static str {
    if only_missing_scores {
        " (missing only)"
    } else {
        ""
    }
}

pub fn queued_score_import_progress(
    requested_charts: usize,
    total_packs: usize,
    import_name: &str,
    only_missing_scores: bool,
) -> ScoreImportProgress {
    ScoreImportProgress {
        processed_charts: 0,
        total_charts: requested_charts,
        imported_scores: 0,
        missing_scores: 0,
        failed_requests: 0,
        detail: format!(
            "Queued {requested_charts} chart hashes across {total_packs} pack(s) for {import_name} import{}.",
            score_import_filter_note(only_missing_scores),
        ),
    }
}

pub const fn empty_score_import_summary() -> ScoreBulkImportSummary {
    ScoreBulkImportSummary {
        requested_charts: 0,
        imported_scores: 0,
        missing_scores: 0,
        failed_requests: 0,
        rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
        elapsed_seconds: 0.0,
        canceled: false,
    }
}

pub fn score_import_summary(
    requested_charts: usize,
    imported_scores: usize,
    missing_scores: usize,
    failed_requests: usize,
    import_started: Instant,
    canceled: bool,
) -> ScoreBulkImportSummary {
    ScoreBulkImportSummary {
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        rate_limit_per_second: SCORE_IMPORT_RATE_LIMIT_PER_SECOND,
        elapsed_seconds: import_started.elapsed().as_secs_f32(),
        canceled,
    }
}

pub fn score_import_pack_detail(
    pack_idx: usize,
    total_packs: usize,
    pack_name: &str,
    pack_hits: usize,
    pack_misses: usize,
    pack_failures: usize,
) -> String {
    format!(
        "Pack {}/{}: {pack_name} -> {pack_hits} hit, {pack_misses} missing{}",
        pack_idx + 1,
        total_packs,
        if pack_failures > 0 {
            format!(", {pack_failures} failed")
        } else {
            String::new()
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScoreImportRunEvent {
    Canceled {
        import_name: &'static str,
        pack_idx: usize,
        total_packs: usize,
        processed_charts: usize,
        requested_charts: usize,
    },
    RequestFailed {
        import_name: &'static str,
        chart_hash: String,
        error: String,
    },
    ProgressLog {
        import_name: &'static str,
        username: String,
        processed_charts: usize,
        requested_charts: usize,
        imported_scores: usize,
        missing_scores: usize,
        failed_requests: usize,
    },
    PackComplete {
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArrowCloudBulkChunkResult {
    pub hits: usize,
    pub misses: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArrowCloudBulkImportRunEvent {
    Canceled {
        pack_name: String,
        pack_idx: usize,
        total_packs: usize,
    },
    ChunkSucceeded {
        pack_idx: usize,
        total_packs: usize,
        pack_name: String,
        chunk_len: usize,
        request_elapsed: Duration,
        hits: usize,
        misses: usize,
    },
    RequestFailed {
        detail: String,
    },
    ChunkDetail {
        detail: String,
    },
    PackComplete {
        detail: String,
    },
}

pub fn log_arrowcloud_bulk_import_event(event: ArrowCloudBulkImportRunEvent) {
    match event {
        ArrowCloudBulkImportRunEvent::Canceled {
            pack_name,
            pack_idx,
            total_packs,
        } => debug!(
            "ArrowCloud bulk import canceled at pack {} ({}/{}).",
            pack_name, pack_idx, total_packs
        ),
        ArrowCloudBulkImportRunEvent::ChunkSucceeded {
            pack_idx,
            total_packs,
            pack_name,
            chunk_len,
            request_elapsed,
            hits,
            misses,
        } => debug!(
            "ArrowCloud /v1/retrieve-scores pack={}/{} pack_name='{}' chunk={} took={:.0}ms hits={} misses={}",
            pack_idx + 1,
            total_packs,
            pack_name,
            chunk_len,
            request_elapsed.as_secs_f32() * 1000.0,
            hits,
            misses,
        ),
        ArrowCloudBulkImportRunEvent::RequestFailed { detail } => warn!("{detail}"),
        ArrowCloudBulkImportRunEvent::ChunkDetail { detail }
        | ArrowCloudBulkImportRunEvent::PackComplete { detail } => debug!("{detail}"),
    }
}

pub fn log_score_import_event(event: ScoreImportRunEvent) {
    match event {
        ScoreImportRunEvent::Canceled {
            import_name,
            pack_idx,
            total_packs,
            processed_charts,
            requested_charts,
        } => debug!(
            "{import_name} score import canceled at pack {}/{} after {processed_charts}/{requested_charts} charts.",
            pack_idx + 1,
            total_packs,
        ),
        ScoreImportRunEvent::RequestFailed {
            import_name,
            chart_hash,
            error,
        } => warn!("{import_name} import request failed for chart {chart_hash}: {error}"),
        ScoreImportRunEvent::ProgressLog {
            import_name,
            username,
            processed_charts,
            requested_charts,
            imported_scores,
            missing_scores,
            failed_requests,
        } => debug!(
            "{import_name} import progress for '{username}': {processed_charts}/{requested_charts} charts (imported={imported_scores}, missing={missing_scores}, failed={failed_requests})",
        ),
        ScoreImportRunEvent::PackComplete { detail } => debug!("{detail}"),
    }
}

pub fn run_arrowcloud_bulk_import_pack_groups<F, P, C, E>(
    pack_chart_groups: Vec<(String, Vec<String>)>,
    chunk_size: usize,
    only_missing_scores: bool,
    mut fetch_chunk: F,
    mut on_progress: P,
    should_cancel: C,
    mut on_event: E,
) -> ScoreBulkImportSummary
where
    F: FnMut(&[String]) -> Result<ArrowCloudBulkChunkResult, String>,
    P: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
    E: FnMut(ArrowCloudBulkImportRunEvent),
{
    let requested_charts: usize = pack_chart_groups.iter().map(|(_, h)| h.len()).sum();
    let total_packs = pack_chart_groups.len();
    on_progress(queued_score_import_progress(
        requested_charts,
        total_packs,
        "ArrowCloud bulk",
        only_missing_scores,
    ));
    if requested_charts == 0 {
        return empty_score_import_summary();
    }

    let import_started = Instant::now();
    let mut last_request_started_at: Option<Instant> = None;
    let mut imported_scores = 0usize;
    let mut missing_scores = 0usize;
    let mut failed_requests = 0usize;
    let mut processed_charts = 0usize;
    let mut canceled = false;
    let chunk_size = chunk_size.max(1);

    'packs: for (pack_idx, (pack_name, hashes)) in pack_chart_groups.into_iter().enumerate() {
        let pack_chart_count = hashes.len();
        let mut pack_hits = 0usize;
        let mut pack_misses = 0usize;
        let mut pack_failures = 0usize;

        for chunk in hashes.chunks(chunk_size) {
            if should_cancel() {
                canceled = true;
                on_event(ArrowCloudBulkImportRunEvent::Canceled {
                    pack_name: pack_name.clone(),
                    pack_idx,
                    total_packs,
                });
                break 'packs;
            }
            wait_for_next_score_import_request(last_request_started_at);
            if should_cancel() {
                canceled = true;
                break 'packs;
            }
            last_request_started_at = Some(Instant::now());

            let request_started = Instant::now();
            let detail = match fetch_chunk(chunk) {
                Ok(result) => {
                    let request_elapsed = request_started.elapsed();
                    imported_scores += result.hits;
                    missing_scores += result.misses;
                    pack_hits += result.hits;
                    pack_misses += result.misses;
                    on_event(ArrowCloudBulkImportRunEvent::ChunkSucceeded {
                        pack_idx,
                        total_packs,
                        pack_name: pack_name.clone(),
                        chunk_len: chunk.len(),
                        request_elapsed,
                        hits: result.hits,
                        misses: result.misses,
                    });
                    arrowcloud_bulk_success_detail(
                        pack_idx,
                        total_packs,
                        &pack_name,
                        result.hits,
                        result.misses,
                        request_elapsed,
                    )
                }
                Err(error) => {
                    let request_elapsed = request_started.elapsed();
                    failed_requests += 1;
                    pack_failures += 1;
                    let detail = arrowcloud_bulk_failure_detail(
                        pack_idx,
                        total_packs,
                        &pack_name,
                        chunk.len(),
                        request_elapsed,
                        &error,
                    );
                    on_event(ArrowCloudBulkImportRunEvent::RequestFailed {
                        detail: detail.clone(),
                    });
                    detail
                }
            };

            processed_charts += chunk.len();
            on_progress(ScoreImportProgress {
                processed_charts,
                total_charts: requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
                detail: detail.clone(),
            });
            on_event(ArrowCloudBulkImportRunEvent::ChunkDetail { detail });
        }

        on_event(ArrowCloudBulkImportRunEvent::PackComplete {
            detail: score_import_pack_complete_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_chart_count,
                pack_hits,
                pack_misses,
                pack_failures,
            ),
        });
    }

    score_import_summary(
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        import_started,
        canceled,
    )
}

pub fn run_score_import_pack_groups<F, H, P, C, E>(
    endpoint: ScoreImportEndpoint,
    username: &str,
    pack_chart_groups: Vec<(String, Vec<String>)>,
    only_missing_scores: bool,
    mut fetch_chart: F,
    mut handle_result: H,
    mut on_progress: P,
    should_cancel: C,
    mut on_event: E,
) -> ScoreBulkImportSummary
where
    F: FnMut(&str) -> Result<CachedScoreImportResult, String>,
    H: FnMut(&str, &CachedScoreImportResult),
    P: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
    E: FnMut(ScoreImportRunEvent),
{
    let import_name = endpoint.display_name();
    let requested_charts: usize = pack_chart_groups.iter().map(|(_, h)| h.len()).sum();
    let total_packs = pack_chart_groups.len();
    on_progress(queued_score_import_progress(
        requested_charts,
        total_packs,
        import_name,
        only_missing_scores,
    ));
    if requested_charts == 0 {
        return empty_score_import_summary();
    }

    let import_started = Instant::now();
    let mut last_request_started_at: Option<Instant> = None;
    let mut imported_scores = 0usize;
    let mut missing_scores = 0usize;
    let mut failed_requests = 0usize;
    let mut processed_charts = 0usize;
    let mut canceled = false;

    'packs: for (pack_idx, (pack_name, hashes)) in pack_chart_groups.into_iter().enumerate() {
        let pack_chart_count = hashes.len();
        let mut pack_hits = 0usize;
        let mut pack_misses = 0usize;
        let mut pack_failures = 0usize;

        for chart_hash in &hashes {
            if should_cancel() {
                canceled = true;
                on_event(ScoreImportRunEvent::Canceled {
                    import_name,
                    pack_idx,
                    total_packs,
                    processed_charts,
                    requested_charts,
                });
                break 'packs;
            }
            wait_for_next_score_import_request(last_request_started_at);
            if should_cancel() {
                canceled = true;
                break 'packs;
            }
            last_request_started_at = Some(Instant::now());
            match fetch_chart(chart_hash) {
                Ok(result) => {
                    let imported = result.score.is_some();
                    handle_result(chart_hash, &result);
                    if imported {
                        imported_scores += 1;
                        pack_hits += 1;
                    } else {
                        missing_scores += 1;
                        pack_misses += 1;
                    }
                }
                Err(error) => {
                    failed_requests += 1;
                    pack_failures += 1;
                    on_event(ScoreImportRunEvent::RequestFailed {
                        import_name,
                        chart_hash: chart_hash.clone(),
                        error,
                    });
                }
            }

            processed_charts += 1;
            let detail = score_import_pack_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_hits,
                pack_misses,
                pack_failures,
            );
            on_progress(ScoreImportProgress {
                processed_charts,
                total_charts: requested_charts,
                imported_scores,
                missing_scores,
                failed_requests,
                detail,
            });
            if should_log_score_import_progress(processed_charts, requested_charts) {
                on_event(ScoreImportRunEvent::ProgressLog {
                    import_name,
                    username: username.to_string(),
                    processed_charts,
                    requested_charts,
                    imported_scores,
                    missing_scores,
                    failed_requests,
                });
            }
        }

        on_event(ScoreImportRunEvent::PackComplete {
            detail: score_import_pack_complete_detail(
                pack_idx,
                total_packs,
                &pack_name,
                pack_chart_count,
                pack_hits,
                pack_misses,
                pack_failures,
            ),
        });
    }

    score_import_summary(
        requested_charts,
        imported_scores,
        missing_scores,
        failed_requests,
        import_started,
        canceled,
    )
}

pub struct ValidatedScoreImportInput<'a> {
    pub endpoint: ScoreImportEndpoint,
    pub api_key: &'a str,
    pub username: &'a str,
    pub pack_chart_groups: Vec<(String, Vec<String>)>,
    pub only_missing_scores: bool,
}

pub fn run_validated_score_import<F, S, I, P, C, E>(
    input: ValidatedScoreImportInput<'_>,
    fetch_chart: F,
    mut store_score: S,
    mut store_itl: I,
    on_progress: P,
    should_cancel: C,
    on_event: E,
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>>
where
    F: FnMut(&str) -> Result<CachedScoreImportResult, String>,
    S: FnMut(&str, CachedScore, bool),
    I: FnMut(&str, Option<u32>, Option<u32>),
    P: FnMut(ScoreImportProgress),
    C: Fn() -> bool,
    E: FnMut(ScoreImportRunEvent),
{
    validate_score_import_credentials(input.endpoint, input.api_key, input.username)
        .map_err(|error| error.import_message())?;
    Ok(run_score_import_pack_groups(
        input.endpoint,
        input.username,
        input.pack_chart_groups,
        input.only_missing_scores,
        fetch_chart,
        |chart_hash, result| {
            if result.itl_self_found {
                store_itl(chart_hash, result.itl_self_score, result.itl_self_rank);
            }
            if let Some(score) = result.score {
                store_score(chart_hash, score, result.score_proves_nonquint_ex);
            }
        },
        on_progress,
        should_cancel,
        on_event,
    ))
}

pub fn score_import_pack_complete_detail(
    pack_idx: usize,
    total_packs: usize,
    pack_name: &str,
    pack_chart_count: usize,
    pack_hits: usize,
    pack_misses: usize,
    pack_failures: usize,
) -> String {
    format!(
        "Pack {}/{} complete: {pack_name} ({pack_chart_count} charts -> {pack_hits} hit, {pack_misses} missing{}).",
        pack_idx + 1,
        total_packs,
        if pack_failures > 0 {
            format!(", {pack_failures} failed")
        } else {
            String::new()
        },
    )
}

pub fn arrowcloud_bulk_success_detail(
    pack_idx: usize,
    total_packs: usize,
    pack_name: &str,
    hits: usize,
    misses: usize,
    request_elapsed: Duration,
) -> String {
    format!(
        "Pack {}/{}: {pack_name} -> {hits} hit, {misses} missing ({:.0}ms)",
        pack_idx + 1,
        total_packs,
        request_elapsed.as_secs_f32() * 1000.0,
    )
}

pub fn arrowcloud_bulk_failure_detail(
    pack_idx: usize,
    total_packs: usize,
    pack_name: &str,
    chart_count: usize,
    request_elapsed: Duration,
    error: &str,
) -> String {
    format!(
        "Pack {}/{}: {pack_name} request failed ({} charts, {:.0}ms): {error}",
        pack_idx + 1,
        total_packs,
        chart_count,
        request_elapsed.as_secs_f32() * 1000.0,
    )
}

#[inline(always)]
pub fn should_log_score_import_progress(processed_charts: usize, requested_charts: usize) -> bool {
    processed_charts == requested_charts
        || processed_charts % SCORE_IMPORT_PROGRESS_LOG_EVERY == 0
        || processed_charts == 1
}

#[inline(always)]
pub fn wait_for_next_score_import_request(last_request_started_at: Option<Instant>) {
    let Some(last_started) = last_request_started_at else {
        return;
    };
    let elapsed = last_started.elapsed();
    if elapsed < SCORE_IMPORT_REQUEST_INTERVAL {
        std::thread::sleep(SCORE_IMPORT_REQUEST_INTERVAL - elapsed);
    }
}

pub fn collect_chart_hashes_per_pack_for_import(
    song_packs: &[deadsync_chart::SongPack],
    pack_groups_filter: &[String],
    existing_scores: &HashSet<String>,
) -> Vec<(String, Vec<String>)> {
    let filter_set: HashSet<String> = pack_groups_filter
        .iter()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .collect();

    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for pack in song_packs {
        let group_name = pack.group_name.trim();
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };
        if !filter_set.is_empty() {
            let group_lc = group_name.to_ascii_lowercase();
            let display_lc = display_name.to_ascii_lowercase();
            if !filter_set.contains(&group_lc) && !filter_set.contains(&display_lc) {
                continue;
            }
        }

        let mut hashes = Vec::new();
        for song in &pack.songs {
            for chart in &song.charts {
                push_unique_import_chart_hash(
                    chart.short_hash.as_str(),
                    existing_scores,
                    &mut seen,
                    &mut hashes,
                );
            }
        }
        if !hashes.is_empty() {
            out.push((display_name.to_string(), hashes));
        }
    }
    out
}

#[inline(always)]
fn push_unique_import_chart_hash<'a>(
    raw_chart_hash: &'a str,
    existing_scores: &HashSet<String>,
    seen: &mut HashSet<&'a str>,
    hashes: &mut Vec<String>,
) {
    let chart_hash = raw_chart_hash.trim();
    if chart_hash.is_empty() || existing_scores.contains(chart_hash) {
        return;
    }
    if seen.insert(chart_hash) {
        hashes.push(chart_hash.to_string());
    }
}

#[cfg(feature = "bench-support")]
#[doc(hidden)]
pub fn collect_unique_import_chart_hashes_for_bench<'a>(
    chart_hashes: &[&'a str],
    existing_scores: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::with_capacity(chart_hashes.len());
    let mut hashes = Vec::new();
    for &chart_hash in chart_hashes {
        push_unique_import_chart_hash(chart_hash, existing_scores, &mut seen, &mut hashes);
    }
    hashes
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
pub fn collect_unique_import_chart_hashes_legacy_for_bench(
    chart_hashes: &[&str],
    existing_scores: &HashSet<String>,
) -> Vec<String> {
    let mut seen = HashSet::with_capacity(chart_hashes.len());
    let mut hashes = Vec::new();
    for &chart_hash in chart_hashes {
        let chart_hash = chart_hash.trim();
        if chart_hash.is_empty() || existing_scores.contains(chart_hash) {
            continue;
        }
        if seen.insert(chart_hash.to_string()) {
            hashes.push(chart_hash.to_string());
        }
    }
    hashes
}

pub fn gs_lamp_chart_stats_for_hash(
    song_packs: &[deadsync_chart::SongPack],
    chart_hash: &str,
) -> Option<GsLampChartStats> {
    let chart_hash = chart_hash.trim();
    if chart_hash.is_empty() {
        return None;
    }
    for pack in song_packs {
        for song in &pack.songs {
            for chart in &song.charts {
                if chart.short_hash == chart_hash {
                    return Some(GsLampChartStats {
                        total_steps: chart.stats.total_steps,
                        holds: chart.stats.holds,
                        rolls: chart.stats.rolls,
                    });
                }
            }
        }
    }
    None
}

pub fn imported_score_chart_stats(
    score: &ImportedPlayerScore,
    song_packs: &[deadsync_chart::SongPack],
    chart_hash: &str,
) -> Option<GsLampChartStats> {
    score
        .needs_chart_stats()
        .then(|| gs_lamp_chart_stats_for_hash(song_packs, chart_hash))
        .flatten()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedItlScore {
    pub ex_hundredths: u32,
    pub clear_type: u8,
    pub points: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ItlEvalState {
    pub active: bool,
    pub eligible: bool,
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub reason_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ItlEvalInput<'a> {
    pub chart_no_cmod: bool,
    pub used_cmod: bool,
    pub groovestats_valid: bool,
    pub groovestats_reason_lines: &'a [String],
    pub music_rate: f32,
    pub mines_enabled: bool,
    pub all_timing_windows_enabled: bool,
    pub passed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ItlGameplayEvalInput<'a> {
    pub song_dir: Option<&'a str>,
    pub group_name: Option<&'a str>,
    pub data: &'a ItlFileData,
    pub chart_hash: &'a str,
    pub subtitle: &'a str,
    pub used_cmod: bool,
    pub groovestats_valid: bool,
    pub groovestats_reason_lines: &'a [String],
    pub music_rate: f32,
    pub remove_mask: u8,
    pub disabled_windows: &'a [bool],
    pub passed: bool,
}

#[derive(Debug, Clone)]
pub struct ItlChartSaveInput<'a> {
    pub song_dir: &'a str,
    pub chart_hash: &'a str,
    pub chart_name: &'a str,
    pub chart_type: &'a str,
    pub event_name: &'a str,
    pub judgments: ItlJudgments,
    pub ex_percent: f64,
    pub used_cmod: bool,
    pub chart_no_cmod: bool,
    pub date: String,
}

#[derive(Debug, Clone)]
pub struct ItlChartSaveResult {
    pub needs_write: bool,
    pub progress: ItlEventProgress,
}

#[derive(Debug, Clone)]
pub struct ItlGameplaySavePlayer {
    pub player_idx: usize,
    pub profile_id: String,
    pub song_dir: Option<String>,
    pub event_name: Option<String>,
    pub chart_hash: String,
    pub chart_name: String,
    pub chart_type: String,
    pub subtitle: String,
    pub used_cmod: bool,
    pub groovestats_valid: bool,
    pub groovestats_reason_lines: Vec<String>,
    pub music_rate: f32,
    pub remove_mask: u8,
    pub disabled_windows: [bool; 5],
    pub passed: bool,
    pub judgments: ItlJudgments,
    pub ex_percent: f64,
    pub date: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ItlGameplaySaveSkip<'a> {
    pub player_idx: usize,
    pub profile_id: &'a str,
    pub chart_hash: &'a str,
    pub reason_lines: &'a [String],
}

#[derive(Debug, Clone)]
pub struct ItlGameplaySaveProgress {
    pub player_idx: usize,
    pub progress: ItlEventProgress,
}

#[derive(Clone, Debug, Default)]
pub struct GrooveStatsEvalState {
    pub valid: bool,
    pub reason_lines: Vec<String>,
    pub manual_qr_url: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct GrooveStatsEvalInput<'a> {
    pub chart_type: &'a str,
    pub music_rate: f32,
    pub remove_mask: u8,
    pub insert_mask: u8,
    pub holds_mask: u8,
    pub fail_type_ok: bool,
    pub autoplay_used: bool,
    pub is_course_mode: bool,
    pub course_submit_allowed: bool,
    pub custom_fantastic_window: bool,
    pub custom_fantastic_window_ms: u8,
}

// Mirrors zmod's old-api submit path bit layout from gameplay.rs/player options.
pub const GS_INVALID_REMOVE_MASK: u8 =
    (1u8 << 0) | (1u8 << 2) | (1u8 << 3) | (1u8 << 4) | (1u8 << 5) | (1u8 << 6) | (1u8 << 7);
pub const GS_INVALID_INSERT_MASK: u8 = u8::MAX;
pub const GS_INVALID_HOLDS_MASK: u8 = 1u8 << 3;
pub const GROOVESTATS_REASON_COUNT: usize = 13;

pub fn groovestats_reason_lines(
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

pub fn groovestats_eval_state_from_parts(input: GrooveStatsEvalInput<'_>) -> GrooveStatsEvalState {
    let chart_type = input.chart_type.trim().to_ascii_lowercase();
    let rate = if input.music_rate.is_finite() && input.music_rate > 0.0 {
        input.music_rate
    } else {
        1.0
    };

    let mut checks = [true; GROOVESTATS_REASON_COUNT];
    checks[0] = chart_type.starts_with("dance") || chart_type.starts_with("pump");
    checks[1] = !chart_type.contains("solo");
    checks[2] = !input.is_course_mode || input.course_submit_allowed;
    checks[3] = true;
    checks[4] = true;
    checks[5] = true;
    checks[6] = !input.custom_fantastic_window;
    checks[7] = (1.0..=3.0).contains(&rate);
    checks[8] = (input.remove_mask & GS_INVALID_REMOVE_MASK) == 0;
    checks[9] = (input.insert_mask & GS_INVALID_INSERT_MASK) == 0;
    checks[10] = input.fail_type_ok;
    checks[11] = !input.autoplay_used;
    checks[12] = true;
    if (input.holds_mask & GS_INVALID_HOLDS_MASK) != 0 {
        checks[8] = false;
    }

    let mut bad = Vec::with_capacity(1);
    if input.custom_fantastic_window {
        bad.push(format!(
            "- Custom Fantastic window ({}ms)",
            input.custom_fantastic_window_ms
        ));
    }

    GrooveStatsEvalState {
        valid: checks.iter().all(|passed| *passed),
        reason_lines: groovestats_reason_lines(&checks, bad.as_slice()),
        manual_qr_url: None,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GrooveStatsGameplayEvalInput {
    pub song_has_lua: bool,
    pub lua_submit_allowed: bool,
    pub song_completed_naturally: bool,
    pub is_failing: bool,
    pub life: f32,
    pub has_fail_time: bool,
    pub course_stage_life_submit_eligible: bool,
}

#[derive(Debug, Clone)]
pub struct GrooveStatsGameplayEvalResult {
    pub state: GrooveStatsEvalState,
    pub should_set_manual_qr_url: bool,
}

pub fn groovestats_eval_state_from_gameplay_parts(
    mut state: GrooveStatsEvalState,
    input: GrooveStatsGameplayEvalInput,
) -> GrooveStatsGameplayEvalResult {
    if state.valid && input.song_has_lua && !input.lua_submit_allowed {
        state.valid = false;
        state.reason_lines.push("simfile relies on lua".to_string());
        return GrooveStatsGameplayEvalResult {
            state,
            should_set_manual_qr_url: false,
        };
    }
    let failed = gameplay_run_failed(input.is_failing, input.has_fail_time);
    let passed = gameplay_run_passed(
        input.song_completed_naturally,
        input.is_failing,
        input.life,
        input.has_fail_time,
    );
    let finished = input.song_completed_naturally || failed;
    if state.valid && !finished {
        state.valid = false;
        state
            .reason_lines
            .push("Only completed stages can be submitted.".to_string());
        return GrooveStatsGameplayEvalResult {
            state,
            should_set_manual_qr_url: false,
        };
    }
    if state.valid && failed {
        state.valid = false;
        state
            .reason_lines
            .push("Only passing scores are submitted.".to_string());
        return GrooveStatsGameplayEvalResult {
            state,
            should_set_manual_qr_url: false,
        };
    }
    if state.valid && !input.course_stage_life_submit_eligible {
        state.valid = false;
        state
            .reason_lines
            .push("Course stage would have failed from normal life.".to_string());
        return GrooveStatsGameplayEvalResult {
            state,
            should_set_manual_qr_url: false,
        };
    }
    GrooveStatsGameplayEvalResult {
        should_set_manual_qr_url: state.valid && passed,
        state,
    }
}

pub fn itl_eval_state_from_parts(input: ItlEvalInput<'_>) -> ItlEvalState {
    let rate = if input.music_rate.is_finite() && input.music_rate > 0.0 {
        input.music_rate
    } else {
        1.0
    };

    let mut reason_lines = Vec::with_capacity(5);
    if !input.groovestats_valid {
        if input.groovestats_reason_lines.is_empty() {
            reason_lines.push("Score is not valid for GrooveStats.".to_string());
        } else {
            reason_lines.extend(input.groovestats_reason_lines.iter().cloned());
        }
    }
    if (rate - 1.0).abs() > 0.0001 {
        reason_lines.push("ITL requires 1.00x music rate.".to_string());
    }
    if !input.mines_enabled {
        reason_lines.push("ITL requires mines to be enabled.".to_string());
    }
    if !input.all_timing_windows_enabled {
        reason_lines.push("ITL requires all timing windows to be enabled.".to_string());
    }
    if !input.passed {
        reason_lines.push("ITL only saves passing scores.".to_string());
    }
    if input.chart_no_cmod && input.used_cmod {
        reason_lines.push("This ITL chart does not allow CMod.".to_string());
    }

    ItlEvalState {
        active: true,
        eligible: reason_lines.is_empty(),
        chart_no_cmod: input.chart_no_cmod,
        used_cmod: input.used_cmod,
        reason_lines,
    }
}

pub fn itl_eval_state_from_gameplay_context(input: ItlGameplayEvalInput<'_>) -> ItlEvalState {
    let Some(song_dir) = input.song_dir else {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod: input.used_cmod,
            reason_lines: Vec::new(),
        };
    };
    if !itl_song_matches_context(Some(song_dir), input.group_name, input.data) {
        return ItlEvalState {
            active: false,
            eligible: false,
            chart_no_cmod: false,
            used_cmod: input.used_cmod,
            reason_lines: Vec::new(),
        };
    }

    let prev = input.data.hash_map.get(input.chart_hash);
    let chart_no_cmod = itl_chart_no_cmod(input.subtitle, prev);
    let mines_enabled = (input.remove_mask & (1u8 << 1)) == 0;
    let all_timing_windows_enabled = itl_timing_windows_all_enabled(input.disabled_windows);
    itl_eval_state_from_parts(ItlEvalInput {
        chart_no_cmod,
        used_cmod: input.used_cmod,
        groovestats_valid: input.groovestats_valid,
        groovestats_reason_lines: input.groovestats_reason_lines,
        music_rate: input.music_rate,
        mines_enabled,
        all_timing_windows_enabled,
        passed: input.passed,
    })
}

pub fn itl_should_warn_cmod_context(
    cached_no_cmod: Option<bool>,
    group_name: Option<&str>,
    subtitle: &str,
) -> bool {
    if let Some(no_cmod) = cached_no_cmod {
        return no_cmod;
    }
    group_name.is_some_and(|group_name| {
        itl_group_name_matches(group_name) && itl_chart_no_cmod(subtitle, None)
    })
}

pub fn itl_current_score_hundredths_for_submit(
    input: ItlScoreCalcInput<'_>,
    disabled_windows: &[bool],
) -> Option<u32> {
    itl_timing_windows_all_enabled(disabled_windows).then(|| itl_current_score_hundredths(input))
}

pub fn save_itl_chart_result(
    data: &mut ItlFileData,
    input: ItlChartSaveInput<'_>,
) -> ItlChartSaveResult {
    let prev_totals = itl_point_totals(data);
    let path_changed = data
        .path_map
        .get(input.song_dir)
        .is_none_or(|hash| !hash.eq_ignore_ascii_case(input.chart_hash));
    if path_changed {
        data.path_map
            .insert(input.song_dir.to_string(), input.chart_hash.to_string());
    }

    let prev = data.hash_map.get(input.chart_hash).cloned();
    let (passing_points, max_scoring_points) = parse_itl_points(input.chart_name)
        .or_else(|| {
            prev.as_ref()
                .map(|entry| (entry.passing_points, entry.max_scoring_points))
        })
        .unwrap_or((0, 0));
    let max_points = passing_points.saturating_add(max_scoring_points);
    let current_run_ex = ex_hundredths(input.ex_percent);
    let new_entry = ItlHashEntry {
        judgments: input.judgments.clone(),
        ex: current_run_ex,
        clear_type: itl_clear_type(&input.judgments),
        points: itl_points_for_song(passing_points, max_scoring_points, input.ex_percent),
        used_cmod: input.used_cmod,
        date: input.date,
        no_cmod: input.chart_no_cmod,
        passing_points,
        max_scoring_points,
        max_points,
        rank: None,
        steps_type: itl_steps_type_from_chart_type(input.chart_type).to_string(),
        passes: prev
            .as_ref()
            .map_or(1, |entry| entry.passes.saturating_add(1)),
    };

    let mut needs_write = path_changed;
    let mut best_changed = false;
    match data.hash_map.get_mut(input.chart_hash) {
        None => {
            data.hash_map
                .insert(input.chart_hash.to_string(), new_entry.clone());
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
                existing.steps_type.clone_from(&new_entry.steps_type);
                needs_write = true;
            }

            let ex_improved = new_entry.ex > existing.ex;
            let ex_tied = new_entry.ex == existing.ex;
            if ex_improved {
                existing.ex = new_entry.ex;
                existing.points = new_entry.points;
                existing.judgments.clone_from(&new_entry.judgments);
                needs_write = true;
                best_changed = true;
            } else if ex_tied && itl_judgments_better(&new_entry.judgments, &existing.judgments) {
                existing.judgments.clone_from(&new_entry.judgments);
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
                existing.date.clone_from(&new_entry.date);
                existing.no_cmod = new_entry.no_cmod;
                existing.passing_points = new_entry.passing_points;
                existing.max_scoring_points = new_entry.max_scoring_points;
                existing.max_points = new_entry.max_points;
            }
        }
    }

    itl_rebuild_song_ranks(data);
    let current_totals = itl_point_totals(data);
    let current_entry = data
        .hash_map
        .get(input.chart_hash)
        .cloned()
        .unwrap_or(new_entry);
    let prev_entry = prev.unwrap_or_default();
    let mut progress = ItlEventProgress {
        kind: EventProgressKind::Itl,
        name: itl_event_name_from_group(Some(input.event_name)),
        is_doubles: current_entry.steps_type.eq_ignore_ascii_case("double"),
        score_hundredths: current_run_ex,
        score_delta_hundredths: delta_i32(current_run_ex, prev_entry.ex),
        rate_hundredths: None,
        rate_delta_hundredths: None,
        current_points: current_entry.points,
        point_delta: delta_i32(current_entry.points, prev_entry.points),
        current_ranking_points: current_totals.ranking_points,
        ranking_delta: delta_i32(current_totals.ranking_points, prev_totals.ranking_points),
        current_song_points: current_totals.song_points,
        song_delta: delta_i32(current_totals.song_points, prev_totals.song_points),
        current_ex_points: current_totals.ex_points,
        ex_delta: delta_i32(current_totals.ex_points, prev_totals.ex_points),
        current_total_points: current_totals.total_points,
        total_delta: delta_i32(current_totals.total_points, prev_totals.total_points),
        total_passes: current_entry.passes.max(1),
        clear_type_before: Some(prev_entry.clear_type),
        clear_type_after: Some(current_entry.clear_type),
        stat_improvements: Vec::new(),
        skill_improvements: Vec::new(),
        overlay_pages: Vec::new(),
    };
    progress.overlay_pages = event_progress_overlay_pages(&progress, None, &[]);

    ItlChartSaveResult {
        needs_write,
        progress,
    }
}

pub fn save_itl_gameplay_players<R, W, C, L>(
    players: impl IntoIterator<Item = ItlGameplaySavePlayer>,
    mut read_file: R,
    mut write_file: W,
    mut set_cached_file: C,
    mut log_skip: L,
) -> Vec<ItlGameplaySaveProgress>
where
    R: FnMut(&str) -> ItlFileData,
    W: FnMut(&str, &ItlFileData),
    C: FnMut(&str, ItlFileData),
    L: FnMut(ItlGameplaySaveSkip<'_>),
{
    let mut progress = Vec::new();
    for player in players {
        let chart_hash = player.chart_hash.trim();
        if chart_hash.is_empty() {
            continue;
        }

        let mut data = read_file(player.profile_id.as_str());
        itl_rebuild_song_ranks(&mut data);
        let eval = itl_eval_state_from_gameplay_context(ItlGameplayEvalInput {
            song_dir: player.song_dir.as_deref(),
            group_name: player.event_name.as_deref(),
            data: &data,
            chart_hash,
            subtitle: player.subtitle.as_str(),
            used_cmod: player.used_cmod,
            groovestats_valid: player.groovestats_valid,
            groovestats_reason_lines: player.groovestats_reason_lines.as_slice(),
            music_rate: player.music_rate,
            remove_mask: player.remove_mask,
            disabled_windows: player.disabled_windows.as_slice(),
            passed: player.passed,
        });
        if !eval.active {
            continue;
        }
        if !eval.eligible {
            log_skip(ItlGameplaySaveSkip {
                player_idx: player.player_idx,
                profile_id: player.profile_id.as_str(),
                chart_hash,
                reason_lines: eval.reason_lines.as_slice(),
            });
            continue;
        }

        let Some(song_dir) = player.song_dir.as_deref() else {
            continue;
        };
        let save_result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir,
                chart_hash,
                chart_name: player.chart_name.as_str(),
                chart_type: player.chart_type.as_str(),
                event_name: player.event_name.as_deref().unwrap_or_default(),
                judgments: player.judgments,
                ex_percent: player.ex_percent,
                used_cmod: eval.used_cmod,
                chart_no_cmod: eval.chart_no_cmod,
                date: player.date,
            },
        );
        progress.push(ItlGameplaySaveProgress {
            player_idx: player.player_idx,
            progress: save_result.progress,
        });

        if save_result.needs_write {
            write_file(player.profile_id.as_str(), &data);
            set_cached_file(player.profile_id.as_str(), data);
        }
    }
    progress
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrooveStatsSubmitUiStatus {
    Submitting,
    Submitted,
    TimedOut,
    NetworkError,
    ServerError { http_status: u16 },
    Rejected { reason: RejectReason },
}

impl GrooveStatsSubmitUiStatus {
    #[inline(always)]
    pub const fn can_retry(self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::NetworkError | Self::ServerError { .. }
        )
    }

    #[inline(always)]
    pub const fn is_auto_retryable(self) -> bool {
        matches!(self, Self::TimedOut)
    }

    #[inline(always)]
    pub const fn from_http_status(status_code: u16) -> Self {
        match status_code {
            408 | 504 => Self::TimedOut,
            500..=599 => Self::ServerError {
                http_status: status_code,
            },
            401 | 403 => Self::Rejected {
                reason: RejectReason::Unauthorized,
            },
            404 => Self::Rejected {
                reason: RejectReason::NotFound,
            },
            _ => Self::Rejected {
                reason: RejectReason::InvalidScore,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowCloudSubmitUiStatus {
    Submitting,
    Submitted,
    TimedOut,
    NetworkError,
    ServerError { http_status: u16 },
    Rejected { reason: RejectReason },
}

impl ArrowCloudSubmitUiStatus {
    #[inline(always)]
    pub const fn can_retry(self) -> bool {
        matches!(
            self,
            Self::TimedOut | Self::NetworkError | Self::ServerError { .. }
        )
    }

    #[inline(always)]
    pub const fn is_auto_retryable(self) -> bool {
        matches!(self, Self::TimedOut)
    }

    #[inline(always)]
    pub const fn from_http_status(status_code: u16) -> Self {
        match status_code {
            408 | 504 => Self::TimedOut,
            500..=599 => Self::ServerError {
                http_status: status_code,
            },
            401 | 403 => Self::Rejected {
                reason: RejectReason::Unauthorized,
            },
            404 => Self::Rejected {
                reason: RejectReason::NotFound,
            },
            _ => Self::Rejected {
                reason: RejectReason::InvalidScore,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GrooveStatsSubmitRecordBanner {
    PersonalBest,
    WorldRecord,
    WorldRecordEx,
}

#[inline(always)]
fn submit_result_improved(result: &str) -> bool {
    result.eq_ignore_ascii_case("score-added") || result.eq_ignore_ascii_case("improved")
}

pub fn groovestats_submit_record_banner(
    result: &str,
    show_ex_score: bool,
    gs_self_rank: Option<u32>,
    ex_self_rank: Option<u32>,
    ex_has_entries: bool,
) -> Option<GrooveStatsSubmitRecordBanner> {
    if !submit_result_improved(result) {
        return None;
    }
    if show_ex_score && ex_has_entries {
        return Some(if ex_self_rank == Some(1) {
            GrooveStatsSubmitRecordBanner::WorldRecordEx
        } else {
            GrooveStatsSubmitRecordBanner::PersonalBest
        });
    }
    Some(if gs_self_rank == Some(1) {
        GrooveStatsSubmitRecordBanner::WorldRecord
    } else {
        GrooveStatsSubmitRecordBanner::PersonalBest
    })
}

static GROOVESTATS_SUBMIT_UI_STATUS: LazyLock<Mutex<SubmitUiState<GrooveStatsSubmitUiStatus>>> =
    LazyLock::new(|| Mutex::new(SubmitUiState::default()));
static GROOVESTATS_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

static GROOVESTATS_SUBMIT_EVENT_UI: LazyLock<
    Mutex<SubmitEventUiState<Vec<EventProgress>, GrooveStatsSubmitRecordBanner>>,
> = LazyLock::new(|| Mutex::new(SubmitEventUiState::default()));

static ARROWCLOUD_SUBMIT_UI_STATUS: LazyLock<Mutex<SubmitUiState<ArrowCloudSubmitUiStatus>>> =
    LazyLock::new(|| Mutex::new(SubmitUiState::default()));
static ARROWCLOUD_SUBMIT_UI_TOKEN: AtomicU64 = AtomicU64::new(1);

#[inline(always)]
pub fn groovestats_reset_submit_ui_status(side_index: usize, chart_hash: &str) {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .reset(side_index, chart_hash);
}

#[inline(always)]
pub fn groovestats_set_submit_ui_status(
    side_index: usize,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .set(side_index, chart_hash, token, status);
}

#[inline(always)]
pub fn groovestats_update_submit_ui_status_if_token(
    side_index: usize,
    chart_hash: &str,
    token: u64,
    status: GrooveStatsSubmitUiStatus,
) -> bool {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .update_if_token(side_index, chart_hash, token, status)
}

#[inline(always)]
pub fn groovestats_next_submit_ui_token() -> u64 {
    GROOVESTATS_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
pub fn groovestats_submit_ui_status(
    side_index: usize,
    chart_hash: &str,
) -> Option<GrooveStatsSubmitUiStatus> {
    GROOVESTATS_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .get(side_index, chart_hash)
}

#[inline(always)]
pub fn groovestats_reset_submit_event_ui(side_index: usize, chart_hash: &str) {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .reset(side_index, chart_hash);
}

#[inline(always)]
pub fn groovestats_arm_submit_event_ui(side_index: usize, chart_hash: &str, token: u64) {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .arm(side_index, chart_hash, token);
}

#[inline(always)]
pub fn groovestats_update_submit_event_ui_if_token(
    side_index: usize,
    chart_hash: &str,
    token: u64,
    event_progress: Vec<EventProgress>,
    record_banner: Option<GrooveStatsSubmitRecordBanner>,
) {
    GROOVESTATS_SUBMIT_EVENT_UI.lock().unwrap().update_if_token(
        side_index,
        chart_hash,
        token,
        event_progress,
        record_banner,
    );
}

#[inline(always)]
pub fn groovestats_submit_event_progress(
    side_index: usize,
    chart_hash: &str,
) -> Vec<EventProgress> {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .progress(side_index, chart_hash)
}

#[inline(always)]
pub fn groovestats_submit_record_banner_ui(
    side_index: usize,
    chart_hash: &str,
) -> Option<GrooveStatsSubmitRecordBanner> {
    GROOVESTATS_SUBMIT_EVENT_UI
        .lock()
        .unwrap()
        .banner(side_index, chart_hash)
}

#[inline(always)]
pub fn arrowcloud_reset_submit_ui_status(side_index: usize, chart_hash: &str) {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .reset(side_index, chart_hash);
}

#[inline(always)]
pub fn arrowcloud_set_submit_ui_status(
    side_index: usize,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .set(side_index, chart_hash, token, status);
}

#[inline(always)]
pub fn arrowcloud_update_submit_ui_status_if_token(
    side_index: usize,
    chart_hash: &str,
    token: u64,
    status: ArrowCloudSubmitUiStatus,
) -> bool {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .update_if_token(side_index, chart_hash, token, status)
}

#[inline(always)]
pub fn arrowcloud_next_submit_ui_token() -> u64 {
    ARROWCLOUD_SUBMIT_UI_TOKEN.fetch_add(1, AtomicOrdering::Relaxed)
}

#[inline(always)]
pub fn arrowcloud_submit_ui_status(
    side_index: usize,
    chart_hash: &str,
) -> Option<ArrowCloudSubmitUiStatus> {
    ARROWCLOUD_SUBMIT_UI_STATUS
        .lock()
        .unwrap()
        .get(side_index, chart_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "deadsync-score-{name}-{}-{nanos}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(score: f64) -> LeaderboardEntry {
        LeaderboardEntry {
            rank: 0,
            name: String::new(),
            machine_tag: None,
            score,
            date: String::new(),
            is_rival: false,
            is_self: false,
            is_fail: false,
        }
    }

    #[test]
    fn submit_event_progress_builds_srpg_and_itl_pages() {
        let input = SubmitEventProgressInput {
            result: "score-added".to_string(),
            score_10000: 9876,
            rate_hundredths: 150,
            itl_score_hundredths: Some(9912),
            srpg: Some(SubmitEventProgressData {
                name: "SRPG".to_string(),
                is_doubles: false,
                score_delta: 1,
                rate_delta: 1,
                top_score_points: 10,
                prev_top_score_points: 4,
                total_passes: 2,
                current_ranking_point_total: 11,
                previous_ranking_point_total: 5,
                current_song_point_total: 12,
                previous_song_point_total: 6,
                current_ex_point_total: 13,
                previous_ex_point_total: 7,
                current_point_total: 14,
                previous_point_total: 8,
                leaderboard: vec![entry(98.76)],
                progress: Some(SubmitProgress {
                    stat_improvements: vec![
                        SubmitStatImprovement {
                            name: "tp".to_string(),
                            gained: 5,
                            current: 12,
                        },
                        SubmitStatImprovement {
                            name: "exp".to_string(),
                            gained: 150,
                            current: 900,
                        },
                    ],
                    skill_improvements: vec!["Gained 150 EXP and reached Level 12".to_string()],
                    ..SubmitProgress::default()
                }),
            }),
            itl: Some(SubmitEventProgressData {
                name: "ITL Online 2026".to_string(),
                is_doubles: true,
                score_delta: 34,
                top_score_points: 100,
                prev_top_score_points: 90,
                total_passes: 3,
                current_ranking_point_total: 200,
                previous_ranking_point_total: 150,
                current_song_point_total: 300,
                previous_song_point_total: 250,
                current_ex_point_total: 400,
                previous_ex_point_total: 350,
                current_point_total: 900,
                previous_point_total: 750,
                leaderboard: vec![entry(99.12)],
                progress: Some(SubmitProgress {
                    stat_improvements: vec![
                        SubmitStatImprovement {
                            name: "clearType".to_string(),
                            gained: 1,
                            current: 4,
                        },
                        SubmitStatImprovement {
                            name: "grade".to_string(),
                            gained: 1,
                            current: 2,
                        },
                    ],
                    quests_completed: vec![SubmitQuest {
                        title: "Unlock Song".to_string(),
                        rewards: vec![SubmitQuestReward {
                            reward_type: "pack".to_string(),
                            description: "New pack".to_string(),
                        }],
                    }],
                    achievements_completed: vec![SubmitAchievement {
                        title: "Milestone".to_string(),
                        rewards: vec![SubmitAchievementReward {
                            tier: "2".to_string(),
                            requirements: vec!["Play one chart".to_string()],
                            title_unlocked: "Champion".to_string(),
                        }],
                    }],
                    skill_improvements: Vec::new(),
                }),
                ..SubmitEventProgressData::default()
            }),
        };

        let progress = event_progress_from_submit(&input);

        assert_eq!(progress.len(), 2);
        assert_eq!(progress[0].kind, EventProgressKind::Srpg);
        assert_eq!(progress[0].score_delta_hundredths, 9876);
        assert_eq!(progress[0].rate_delta_hundredths, Some(150));
        assert!(matches!(
            &progress[0].overlay_pages[0],
            EventOverlayPage::Text(text)
                if text.contains("Skill Improvements")
                    && text.contains("+5 TP")
                    && text.contains("+150 EXP")
                    && text.contains("Gained 150 EXP and reached Level 12")
        ));

        assert_eq!(progress[1].kind, EventProgressKind::Itl);
        assert_eq!(progress[1].clear_type_before, Some(3));
        assert_eq!(progress[1].clear_type_after, Some(4));
        assert!(matches!(
            &progress[1].overlay_pages[0],
            EventOverlayPage::Text(text)
                if text.contains("Clear Type: FEC >>> FFC") && text.contains("New Quint!")
        ));
        assert!(matches!(
            &progress[1].overlay_pages[1],
            EventOverlayPage::Text(text)
                if text.contains("Completed \"Unlock Song\"!") && text.contains("PACK:")
        ));
        assert!(matches!(
            &progress[1].overlay_pages[2],
            EventOverlayPage::Text(text)
                if text.contains("Completed the \"Milestone\" Achievement!")
                    && text.contains("Unlocked the \"Champion\" Title!")
        ));
        assert!(matches!(
            progress[1].overlay_pages.last(),
            Some(EventOverlayPage::Leaderboard(entries)) if entries.len() == 1
        ));
    }

    #[test]
    fn grade_sprite_indices_are_stable() {
        assert_eq!(Grade::Quint.to_sprite_state(), 0);
        assert_eq!(Grade::Tier01.to_sprite_state(), 1);
        assert_eq!(Grade::Failed.to_sprite_state(), 18);
    }

    #[test]
    fn import_chart_hash_deduplication_preserves_order_and_filtering() {
        let existing_scores = HashSet::from(["existing".to_string()]);
        let chart_hashes = [
            " first ", "existing", "second", "first", "", "  ", "third", "second ",
        ];
        let expected = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ];
        let mut seen = HashSet::new();
        let mut current = Vec::new();
        for chart_hash in chart_hashes {
            push_unique_import_chart_hash(chart_hash, &existing_scores, &mut seen, &mut current);
        }
        assert_eq!(current, expected);
        assert_eq!(
            current,
            collect_unique_import_chart_hashes_legacy_for_bench(&chart_hashes, &existing_scores)
        );
    }

    #[test]
    fn score_thresholds_match_itg_tiers() {
        assert_eq!(score_to_grade(10000.0), Grade::Tier01);
        assert_eq!(score_to_grade(9900.0), Grade::Tier02);
        assert_eq!(score_to_grade(9400.0), Grade::Tier05);
        assert_eq!(score_to_grade(5499.0), Grade::Tier17);
    }

    #[test]
    fn quint_promotion_preserves_failed_runs() {
        assert_eq!(promote_quint_grade(Grade::Tier01, 100.0), Grade::Quint);
        assert_eq!(promote_quint_grade(Grade::Failed, 100.0), Grade::Failed);
    }

    #[test]
    fn grade_storage_codes_round_trip_stable_variants() {
        for (grade, code) in [
            (Grade::Quint, 0),
            (Grade::Tier01, 1),
            (Grade::Tier02, 2),
            (Grade::Tier03, 3),
            (Grade::Tier04, 4),
            (Grade::Tier05, 5),
            (Grade::Tier06, 6),
            (Grade::Tier07, 7),
            (Grade::Tier08, 8),
            (Grade::Tier09, 9),
            (Grade::Tier10, 10),
            (Grade::Tier11, 11),
            (Grade::Tier12, 12),
            (Grade::Tier13, 13),
            (Grade::Tier14, 14),
            (Grade::Tier15, 15),
            (Grade::Tier16, 16),
            (Grade::Tier17, 17),
            (Grade::Failed, 18),
        ] {
            assert_eq!(grade_to_code(grade), code);
            assert_eq!(grade_from_code(code), grade);
        }
        assert_eq!(grade_from_code(255), Grade::Failed);
    }

    #[test]
    fn cached_score_clears_lamp_fields_for_failed_runs() {
        let score = cached_score(Grade::Failed, 0.91, Some(3), Some(2));
        assert_eq!(score.grade, Grade::Failed);
        assert_eq!(score.score_percent, 0.91);
        assert_eq!(score.lamp_index, None);
        assert_eq!(score.lamp_judge_count, None);
    }

    #[test]
    fn itg_score_order_prefers_richer_tied_score() {
        let quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };
        let quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let bland_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };

        assert!(is_better_itg(&quint, &quad));
        assert!(is_better_itg(&quad, &bland_quad));
        assert!(!is_better_itg(&bland_quad, &quad));
    }

    #[test]
    fn stale_quint_replacement_requires_nonperfect_ex() {
        let stale_quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let corrected_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };

        assert!(replaces_stale_quint(&corrected_quad, &stale_quint, true));
        assert!(!replaces_stale_quint(&corrected_quad, &stale_quint, false));
        assert!(!is_better_itg(&corrected_quad, &stale_quint));
    }

    #[test]
    fn failed_score_override_preserves_known_failed_run() {
        let failed = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let passed = CachedScore {
            grade: Grade::Tier17,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };

        assert_eq!(failed_score_override(&failed, &passed), Some(failed));
        assert_eq!(failed_score_override(&passed, &failed), Some(failed));
    }

    #[test]
    fn cached_gs_replacement_policy_allows_score_corrections() {
        let stale_quint = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        let corrected_quad = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: None,
        };
        assert!(should_replace_cached_gs_score(
            &corrected_quad,
            &stale_quint,
            true
        ));
        assert!(!should_replace_cached_gs_score(
            &corrected_quad,
            &stale_quint,
            false
        ));

        let passed = CachedScore {
            grade: Grade::Tier17,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let failed = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(should_replace_cached_gs_score(&failed, &passed, false));

        let better_existing = CachedScore {
            grade: Grade::Tier10,
            score_percent: 0.91,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let worse = CachedScore {
            grade: Grade::Tier12,
            score_percent: 0.75,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(!should_replace_cached_gs_score(
            &worse,
            &better_existing,
            false
        ));
    }

    #[test]
    fn gs_cached_score_recomputes_stale_quint_from_lamp() {
        let stale = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };
        assert_eq!(fix_gs_cached_score(stale).grade, Grade::Tier01);

        let quint_lamp = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(0),
            lamp_judge_count: None,
        };
        assert_eq!(fix_gs_cached_score(quint_lamp).grade, Grade::Quint);
    }

    #[test]
    fn score_profile_paths_match_profile_score_layout() {
        let paths = ScoreProfilePaths::new(PathBuf::from("Profiles/A"));
        assert_eq!(paths.scores_dir(), PathBuf::from("Profiles/A/scores"));
        assert_eq!(paths.gs_dir(), PathBuf::from("Profiles/A/scores/gs"));
        assert_eq!(
            paths.gs_chart_dir("abcdef"),
            PathBuf::from("Profiles/A/scores/gs/ab")
        );
        assert_eq!(
            paths.gs_index_path(),
            PathBuf::from("Profiles/A/scores/gs/index.bin")
        );
        assert_eq!(paths.ac_dir(), PathBuf::from("Profiles/A/scores/ac"));
        assert_eq!(
            paths.ac_index_path(),
            PathBuf::from("Profiles/A/scores/ac/index.bin")
        );
        assert_eq!(paths.local_dir(), PathBuf::from("Profiles/A/scores/local"));
        assert_eq!(
            paths.local_index_path(),
            PathBuf::from("Profiles/A/scores/local/index.bin")
        );
    }

    #[test]
    fn score_cache_states_insert_loaded_profiles_once() {
        let first = CachedScore {
            grade: Grade::Tier03,
            score_percent: 0.96,
            lamp_index: Some(3),
            lamp_judge_count: None,
        };
        let second = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(2),
        };

        let mut gs = GsScoreCacheState::default();
        let mut first_gs = HashMap::new();
        first_gs.insert("chart".to_string(), first);
        let mut second_gs = HashMap::new();
        second_gs.insert("chart".to_string(), second);
        assert!(!gs.profile_is_loaded("profile"));
        gs.insert_loaded_profile("profile", first_gs);
        gs.insert_loaded_profile("profile", second_gs);
        assert!(gs.profile_is_loaded("profile"));
        assert_eq!(gs.get_profile_score("profile", "chart"), Some(first));

        let mut ac = AcScoreCacheState::default();
        let mut first_ac = HashMap::new();
        first_ac.insert(
            "chart".to_string(),
            ArrowCloudScores {
                itg: Some(ArrowCloudScore {
                    score_percent: 0.9,
                    server_grade: None,
                    played_at: Some(Utc.timestamp_opt(1, 0).unwrap()),
                    play_id: Some(1),
                    is_fail: false,
                }),
                ..ArrowCloudScores::default()
            },
        );
        let mut second_ac = HashMap::new();
        second_ac.insert("chart".to_string(), ArrowCloudScores::default());
        assert!(!ac.profile_is_loaded("profile"));
        ac.insert_loaded_profile("profile", first_ac);
        ac.insert_loaded_profile("profile", second_ac);
        assert!(ac.profile_is_loaded("profile"));
        assert!(
            ac.get_profile_scores("profile", "chart")
                .unwrap()
                .itg
                .is_some()
        );

        let mut local = LocalScoreCacheState::default();
        let mut first_index = LocalScoreIndex::default();
        first_index.best_itg.insert("chart".to_string(), first);
        let mut second_index = LocalScoreIndex::default();
        second_index.best_itg.insert("chart".to_string(), second);
        assert!(!local.profile_is_loaded("profile"));
        local.insert_loaded_profile("profile", first_index);
        local.insert_loaded_profile("profile", second_index);
        assert!(local.profile_is_loaded("profile"));
        assert_eq!(local.get_profile_itg_score("profile", "chart"), Some(first));
    }

    #[test]
    fn load_gs_score_index_or_scan_normalizes_existing_index() {
        let dir = test_dir("gs-index-normalize");
        let paths = ScoreProfilePaths::new(&dir);
        let stale = CachedScore {
            grade: Grade::Quint,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };
        let mut map = HashMap::new();
        map.insert("chart".to_string(), stale);
        save_gs_score_index_file(&paths.gs_index_path(), &map).unwrap();

        let (loaded, write_error) = load_gs_score_index_or_scan(&paths);
        assert!(write_error.is_none());
        assert_eq!(loaded["chart"].grade, Grade::Tier01);

        let (persisted, changed) = load_gs_score_index_file(&paths.gs_index_path()).unwrap();
        assert!(!changed);
        assert_eq!(persisted["chart"].grade, Grade::Tier01);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_gs_score_index_or_scan_rebuilds_missing_index_from_files() {
        let dir = test_dir("gs-index-scan");
        let paths = ScoreProfilePaths::new(&dir);
        let score = CachedScore {
            grade: Grade::Tier03,
            score_percent: 0.96,
            lamp_index: Some(3),
            lamp_judge_count: Some(4),
        };
        write_gs_score_entry_file(&paths.gs_chart_dir("abcdef"), "abcdef", score, "AAA", 123)
            .unwrap();

        let (loaded, write_error) = load_gs_score_index_or_scan(&paths);
        assert!(write_error.is_none());
        assert_eq!(loaded.get("abcdef").copied(), Some(score));
        assert!(paths.gs_index_path().exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn gs_score_entry_roundtrip_preserves_lamp_judge_count() {
        let score = CachedScore {
            grade: Grade::Tier01,
            score_percent: 1.0,
            lamp_index: Some(1),
            lamp_judge_count: Some(3),
        };

        let entry = gs_score_entry_from_cached(score, "Player", 1234);
        assert_eq!(entry.grade_code, grade_to_code(Grade::Tier01));
        assert_eq!(entry.username, "Player");
        assert_eq!(entry.fetched_at_ms, 1234);

        let bytes = encode_gs_score_entry(&entry).expect("GS entry should encode");
        let decoded = decode_gs_score_entry(&bytes).expect("GS entry should decode");
        assert_eq!(decoded, entry);
        assert_eq!(cached_score_from_gs_entry(&decoded), score);
    }

    #[test]
    fn gs_score_entry_decode_accepts_legacy_payload_without_judge_count() {
        let bytes = bincode::encode_to_vec(
            GsScoreEntryV1 {
                score_percent: 0.985,
                grade_code: grade_to_code(Grade::Tier03),
                lamp_index: Some(2),
                username: "Legacy".to_string(),
                fetched_at_ms: 4321,
            },
            bincode::config::standard(),
        )
        .expect("legacy GS entry should encode");

        let entry = decode_gs_score_entry(&bytes).expect("legacy GS entry should decode");
        assert_eq!(entry.score_percent, 0.985);
        assert_eq!(entry.grade_code, grade_to_code(Grade::Tier03));
        assert_eq!(entry.lamp_index, Some(2));
        assert_eq!(entry.lamp_judge_count, None);
        assert_eq!(entry.username, "Legacy");
        assert_eq!(entry.fetched_at_ms, 4321);
    }

    #[test]
    fn local_ex_grade_promotion_ignores_stale_quint_without_perfect_ex() {
        assert_eq!(fix_local_ex_grade(Grade::Quint, 99.71), Grade::Tier01);
        assert_eq!(fix_local_ex_grade(Grade::Tier01, 100.0), Grade::Quint);
        assert_eq!(fix_local_ex_grade(Grade::Failed, 100.0), Grade::Failed);
    }

    #[test]
    fn local_lamp_uses_white_fantastics_for_quad() {
        assert_eq!(
            compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Tier01, Some(5)),
            (Some(1), Some(5))
        );
        assert_eq!(
            compute_local_lamp([12, 0, 0, 0, 0, 0], Grade::Quint, Some(0)),
            (Some(0), None)
        );
    }

    #[test]
    fn cached_score_from_local_header_treats_fail_time_as_failed() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 0.9482,
            grade_code: grade_to_code(Grade::Tier06),
            lamp_index: Some(4),
            lamp_judge_count: Some(2),
            ex_score_percent: 92.0,
            hard_ex_score_percent: 88.0,
            judgment_counts: [0; 6],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: Some(12.0),
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Failed);
        assert_eq!(cached.score_percent, 0.9482);
        assert_eq!(cached.lamp_index, None);
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_local_header_downgrades_stale_quint() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Quint),
            lamp_index: Some(0),
            lamp_judge_count: None,
            ex_score_percent: 99.71,
            hard_ex_score_percent: 99.71,
            judgment_counts: [320, 0, 0, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Tier01);
        assert_eq!(cached.lamp_index, Some(1));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn cached_score_from_local_header_promotes_perfect_ex_to_quint() {
        let header = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 0,
            music_rate: 1.0,
            score_percent: 1.0,
            grade_code: grade_to_code(Grade::Tier01),
            lamp_index: Some(1),
            lamp_judge_count: Some(2),
            ex_score_percent: 100.0,
            hard_ex_score_percent: 100.0,
            judgment_counts: [320, 0, 0, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };

        let cached = cached_score_from_local_header(&header);

        assert_eq!(cached.grade, Grade::Quint);
        assert_eq!(cached.lamp_index, Some(0));
        assert_eq!(cached.lamp_judge_count, None);
    }

    #[test]
    fn local_score_entry_roundtrip_decodes_header_prefix() {
        let entry = LocalScoreEntry {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 1234,
            music_rate: 1.1,
            score_percent: 0.9876,
            grade_code: grade_to_code(Grade::Tier03),
            lamp_index: Some(2),
            lamp_judge_count: Some(4),
            ex_score_percent: 98.25,
            hard_ex_score_percent: 97.75,
            judgment_counts: [100, 4, 3, 2, 1, 0],
            holds_held: 5,
            holds_total: 6,
            rolls_held: 7,
            rolls_total: 8,
            mines_avoided: 9,
            mines_total: 10,
            hands_achieved: 11,
            fail_time: None,
            beat0_time_ns: -250_000_000,
            replay: vec![LocalReplayEdge::new(
                1_500_000_000,
                2,
                true,
                InputSource::Gamepad,
            )],
        };

        let bytes = encode_local_score_entry(&entry).expect("local score should encode");
        let decoded = decode_local_score_entry(&bytes).expect("local score should decode");
        let header = decode_local_score_header(&bytes).expect("local header should decode");

        assert_eq!(decoded, entry);
        assert_eq!(header, entry.header());
        assert_eq!(decoded.replay[0].input_source(), InputSource::Gamepad);
    }

    fn test_local_score_entry(played_at_ms: i64, score_percent: f64) -> LocalScoreEntry {
        LocalScoreEntry {
            version: LOCAL_SCORE_VERSION,
            played_at_ms,
            music_rate: 1.0,
            score_percent,
            grade_code: grade_to_code(Grade::Tier03),
            lamp_index: Some(2),
            lamp_judge_count: Some(4),
            ex_score_percent: score_percent * 100.0,
            hard_ex_score_percent: score_percent * 100.0,
            judgment_counts: [100, 4, 3, 2, 1, 0],
            holds_held: 5,
            holds_total: 6,
            rolls_held: 7,
            rolls_total: 8,
            mines_avoided: 9,
            mines_total: 10,
            hands_achieved: 11,
            fail_time: None,
            beat0_time_ns: 0,
            replay: Vec::new(),
        }
    }

    #[test]
    fn local_score_append_updates_disk_index() {
        let dir = test_dir("local-append-index");
        let paths = ScoreProfilePaths::new(&dir);
        let mut entry = test_local_score_entry(1234, 0.9876);
        let append = write_local_score_entry_for_profile(&paths, "deadbeef", &mut entry).unwrap();

        save_local_score_index_after_append(&paths, "deadbeef", &append.header, None).unwrap();

        let index = load_local_score_index_file(&paths.local_index_path()).unwrap();
        assert_eq!(append.header, entry.header());
        assert_eq!(
            append.cached_score,
            cached_score_from_local_header(&entry.header())
        );
        assert_eq!(index.best_itg["deadbeef"], append.cached_score);
        assert!(append.path.is_file());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_local_scores_with_writer_counts_progress_and_cancel() {
        let mut scores = vec![
            ("one".to_string(), test_local_score_entry(1, 0.9)),
            ("two".to_string(), test_local_score_entry(2, 0.95)),
            ("three".to_string(), test_local_score_entry(3, 0.97)),
        ];
        let mut progress = Vec::new();
        let mut attempted = Vec::new();
        let attempts = std::cell::Cell::new(0);

        let (written, canceled) = import_local_scores_with_writer(
            &mut scores,
            |done, total| progress.push((done, total)),
            || attempts.get() == 2,
            |chart_hash, _entry| {
                attempts.set(attempts.get() + 1);
                attempted.push(chart_hash.to_string());
                chart_hash != "two"
            },
        );

        assert_eq!(written, 1);
        assert!(canceled);
        assert_eq!(attempted, ["one", "two"]);
        assert_eq!(progress, [(1, 3), (2, 3)]);
    }

    #[test]
    fn local_score_index_update_tracks_each_score_type() {
        let older = LocalScoreHeader {
            version: LOCAL_SCORE_VERSION,
            played_at_ms: 1,
            music_rate: 1.0,
            score_percent: 0.9500,
            grade_code: grade_to_code(Grade::Tier06),
            lamp_index: Some(3),
            lamp_judge_count: Some(2),
            ex_score_percent: 94.0,
            hard_ex_score_percent: 91.0,
            judgment_counts: [100, 1, 2, 0, 0, 0],
            holds_held: 0,
            holds_total: 0,
            rolls_held: 0,
            rolls_total: 0,
            mines_avoided: 0,
            mines_total: 0,
            hands_achieved: 0,
            fail_time: None,
            beat0_time_ns: 0,
        };
        let mut newer = older;
        newer.score_percent = 0.9900;
        newer.grade_code = grade_to_code(Grade::Tier02);
        newer.ex_score_percent = 93.0;
        newer.hard_ex_score_percent = 96.0;
        newer.music_rate = 1.25;

        let mut failed_faster = newer;
        failed_faster.music_rate = 2.0;
        failed_faster.grade_code = grade_to_code(Grade::Failed);
        failed_faster.fail_time = Some(10.0);

        let mut passed_faster = newer;
        passed_faster.score_percent = 0.7500;
        passed_faster.grade_code = grade_to_code(Grade::Tier12);
        passed_faster.music_rate = 1.5;

        let mut index = LocalScoreIndex::default();
        update_local_score_index(&mut index, "deadbeef", &older);
        update_local_score_index(&mut index, "deadbeef", &newer);
        update_local_score_index(&mut index, "deadbeef", &failed_faster);
        update_local_score_index(&mut index, "deadbeef", &passed_faster);

        assert_eq!(index.best_itg["deadbeef"].score_percent, 0.9900);
        assert_eq!(index.best_ex["deadbeef"].percent, 94.0);
        assert_eq!(index.best_hard_ex["deadbeef"].percent, 96.0);
        assert_eq!(index.best_pass_rate["deadbeef"], 150);
    }

    #[test]
    fn local_score_index_roundtrip_rejects_wrong_version() {
        let mut index = LocalScoreIndex::default();
        index.best_itg.insert(
            "deadbeef".to_string(),
            cached_score(Grade::Tier04, 0.975, Some(3), Some(2)),
        );
        index.best_pass_rate.insert("deadbeef".to_string(), 125);

        let bytes = encode_local_score_index(&index).expect("local index should encode");
        let decoded = decode_local_score_index(&bytes).expect("local index should decode");
        assert_eq!(decoded, index);

        let wrong_version = bincode::encode_to_vec(
            LocalScoreIndexFile {
                version: LOCAL_SCORE_INDEX_VERSION + 1,
                index,
            },
            bincode::config::standard(),
        )
        .expect("wrong-version local index should encode");
        assert_eq!(decode_local_score_index(&wrong_version), None);
    }

    #[test]
    fn leaderboard_rank_places_current_run_before_equal_scores() {
        let entries = [entry(9800.0), entry(9750.0), entry(9700.0)];
        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
        assert_eq!(leaderboard_rank_for_score(&entries, f64::NAN), None);
    }

    #[test]
    fn leaderboard_rank_uses_scores_not_identity() {
        let mut top = entry(9999.0);
        top.name = "AAA".to_string();
        let mut target = entry(9750.0);
        target.name = "BBB".to_string();
        let entries = [top, target];

        assert_eq!(leaderboard_rank_for_score(&entries, 0.975), Some(2));
    }

    #[test]
    fn leaderboard_identity_helpers_match_self_and_username() {
        assert!(leaderboard_username_matches("PerfectTaste", "perfecttaste"));
        assert!(!leaderboard_username_matches("PerfectTaste", " "));
        assert_eq!(leaderboard_nonzero_rank(25), Some(25));
        assert_eq!(leaderboard_nonzero_rank(0), None);
    }

    #[test]
    fn leaderboard_score_10000_clamps_and_drops_failed_scores() {
        assert_eq!(leaderboard_score_10000(10_001.2, false), Some(10_000.0));
        assert_eq!(leaderboard_score_10000(-1.0, false), Some(0.0));
        assert_eq!(leaderboard_score_10000(9_500.0, true), None);
        assert_eq!(leaderboard_score_10000(f64::NAN, false), None);
    }

    #[test]
    fn score_import_profile_match_uses_endpoint_rules() {
        assert!(score_import_entry_matches_profile(
            "Someone",
            true,
            ScoreImportEndpoint::ArrowCloud,
            ""
        ));
        assert!(score_import_entry_matches_profile(
            "PerfectTaste",
            false,
            ScoreImportEndpoint::GrooveStats,
            "perfecttaste"
        ));
        assert!(!score_import_entry_matches_profile(
            "PerfectTaste",
            false,
            ScoreImportEndpoint::ArrowCloud,
            "perfecttaste"
        ));
    }

    #[test]
    fn machine_leaderboard_entries_sort_and_limit_local_plays() {
        let entries = machine_leaderboard_entries(
            vec![
                MachineLeaderboardPlay {
                    name: "Bob".to_string(),
                    machine_tag: None,
                    score_percent: 0.99,
                    played_at_ms: 10,
                    is_fail: false,
                },
                MachineLeaderboardPlay {
                    name: "Cara".to_string(),
                    machine_tag: Some("C".to_string()),
                    score_percent: 0.99,
                    played_at_ms: 30,
                    is_fail: true,
                },
                MachineLeaderboardPlay {
                    name: "Alice".to_string(),
                    machine_tag: None,
                    score_percent: 0.97,
                    played_at_ms: 20,
                    is_fail: false,
                },
            ],
            2,
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].rank, 1);
        assert_eq!(entries[0].name, "Cara");
        assert_eq!(entries[0].machine_tag.as_deref(), Some("C"));
        assert_eq!(entries[0].score, 9900.0);
        assert!(entries[0].is_fail);
        assert_eq!(entries[1].rank, 2);
        assert_eq!(entries[1].name, "Bob");
    }

    #[test]
    fn machine_replay_entries_sort_and_drop_invalid_edges() {
        let entries = machine_replay_entries(
            vec![
                MachineReplayPlay {
                    initials: "AAA".to_string(),
                    score_percent: 0.91,
                    played_at_ms: 30,
                    is_fail: false,
                    replay_beat0_time_ns: 0,
                    replay: Vec::new(),
                },
                MachineReplayPlay {
                    initials: "BBB".to_string(),
                    score_percent: 0.98,
                    played_at_ms: 10,
                    is_fail: false,
                    replay_beat0_time_ns: -250_000_000,
                    replay: vec![
                        LocalReplayEdge::new(
                            deadsync_core::song_time::INVALID_SONG_TIME_NS,
                            1,
                            true,
                            InputSource::Keyboard,
                        ),
                        LocalReplayEdge::new(1_250_000_000, 2, false, InputSource::Gamepad),
                    ],
                },
            ],
            1,
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].rank, 1);
        assert_eq!(entries[0].name, "BBB");
        assert_eq!(entries[0].score, 9800.0);
        assert_eq!(entries[0].replay_beat0_time_ns, -250_000_000);
        assert_eq!(entries[0].replay.len(), 1);
        assert_eq!(entries[0].replay[0].lane_index, 2);
        assert_eq!(entries[0].replay[0].source, InputSource::Gamepad);
    }

    #[test]
    fn local_replay_edges_for_player_filters_lanes_and_invalid_times() {
        let edges = local_replay_edges_for_player(
            [
                LocalReplayEdgeInput {
                    event_music_time_ns: 1_000,
                    lane_index: 1,
                    pressed: true,
                    source: InputSource::Keyboard,
                },
                LocalReplayEdgeInput {
                    event_music_time_ns: 2_000,
                    lane_index: 4,
                    pressed: false,
                    source: InputSource::Gamepad,
                },
                LocalReplayEdgeInput {
                    event_music_time_ns: deadsync_core::song_time::INVALID_SONG_TIME_NS,
                    lane_index: 5,
                    pressed: true,
                    source: InputSource::Keyboard,
                },
            ],
            1,
            2,
            8,
            4,
        );

        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].event_music_time_ns, 2_000);
        assert_eq!(edges[0].lane, 0);
        assert!(!edges[0].pressed);
        assert_eq!(edges[0].input_source(), InputSource::Gamepad);
    }

    #[test]
    fn local_score_gameplay_save_decision_skips_autoplay_and_invalid_scores() {
        let counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        assert_eq!(
            local_score_gameplay_save_decision(LocalScoreGameplaySaveInput {
                autoplay_used: true,
                score_valid: true,
                invalid_reasons: &[],
                scoring_counts: &counts,
                holds_held_for_score: 0,
                rolls_held_for_score: 0,
                mines_hit_for_score: 0,
                possible_grade_points: 0,
            }),
            LocalScoreGameplaySaveDecision::SkipAutoplay
        );

        let reasons = vec!["profile uses CMod", "profile uses NoMines"];
        assert_eq!(
            local_score_gameplay_save_decision(LocalScoreGameplaySaveInput {
                autoplay_used: false,
                score_valid: false,
                invalid_reasons: &reasons,
                scoring_counts: &counts,
                holds_held_for_score: 0,
                rolls_held_for_score: 0,
                mines_hit_for_score: 0,
                possible_grade_points: 0,
            }),
            LocalScoreGameplaySaveDecision::SkipInvalid {
                detail: "profile uses CMod; profile uses NoMines".to_string()
            }
        );

        assert_eq!(
            local_score_gameplay_save_decision(LocalScoreGameplaySaveInput {
                autoplay_used: false,
                score_valid: false,
                invalid_reasons: &[],
                scoring_counts: &counts,
                holds_held_for_score: 0,
                rolls_held_for_score: 0,
                mines_hit_for_score: 0,
                possible_grade_points: 0,
            }),
            LocalScoreGameplaySaveDecision::SkipInvalid {
                detail: "ranking-invalid modifiers were used".to_string()
            }
        );
    }

    #[test]
    fn local_score_gameplay_save_decision_calculates_score_percent() {
        let mut counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)] = 80;
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)] = 20;

        let LocalScoreGameplaySaveDecision::Save { score_percent } =
            local_score_gameplay_save_decision(LocalScoreGameplaySaveInput {
                autoplay_used: false,
                score_valid: true,
                invalid_reasons: &["ignored when valid"],
                scoring_counts: &counts,
                holds_held_for_score: 0,
                rolls_held_for_score: 0,
                mines_hit_for_score: 0,
                possible_grade_points: 500,
            })
        else {
            panic!("valid score should be saved");
        };
        assert!((score_percent - 0.96).abs() < f64::EPSILON);
    }

    #[test]
    fn save_local_gameplay_scores_skips_invalid_and_writes_valid() {
        let mut counts = [0u32; judgment::JUDGE_GRADE_COUNT];
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)] = 80;
        counts[judgment::judge_grade_ix(judgment::JudgeGrade::Excellent)] = 20;
        let notes: [Note; 0] = [];
        let times: [SongTimeNs; 0] = [];
        let hold_times: [Option<SongTimeNs>; 0] = [];
        let mut written = Vec::new();
        let mut skips = Vec::new();
        let player = |player_idx,
                      score_valid,
                      invalid_reasons: Vec<&'static str>|
         -> LocalScoreGameplayPlayer<'_> {
            LocalScoreGameplayPlayer {
                player_idx,
                profile_id: format!("profile-{player_idx}"),
                profile_initials: "AAA",
                chart_hash: "deadbeef",
                invalid_reasons,
                score_valid,
                scoring_counts: &counts,
                holds_held_for_score: 0,
                rolls_held_for_score: 0,
                mines_hit_for_score: 0,
                possible_grade_points: 500,
                song_completed_naturally: true,
                is_failing: false,
                life: 1.0,
                fail_time: None,
                notes: &notes,
                note_times: &times,
                hold_end_times: &hold_times,
                total_steps: 100,
                holds_total: 0,
                rolls_total: 0,
                mines_total: 0,
                counts: [80, 20, 0, 0, 0, 0],
                white_fantastics: Some(80),
                holds_held: 0,
                rolls_held: 0,
                mines_avoided: 0,
                hands_achieved: 0,
                beat0_time_ns: 123,
                replay: Vec::new(),
            }
        };

        save_local_gameplay_scores(
            1234,
            1.0,
            false,
            [
                player(0, false, vec!["profile uses CMod"]),
                player(1, true, Vec::new()),
            ],
            |profile_id, initials, chart_hash, entry| {
                written.push((
                    profile_id.to_string(),
                    initials.to_string(),
                    chart_hash.to_string(),
                    entry.clone(),
                ));
                true
            },
            |skip| skips.push(skip),
        );

        assert_eq!(
            skips,
            vec![LocalScoreGameplaySaveSkip::Invalid {
                player_idx: 0,
                detail: "profile uses CMod".to_string()
            }]
        );
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0, "profile-1");
        assert_eq!(written[0].1, "AAA");
        assert_eq!(written[0].2, "deadbeef");
        assert!((written[0].3.score_percent - 0.96).abs() < f64::EPSILON);
        assert_eq!(written[0].3.played_at_ms, 1234);
        assert_eq!(written[0].3.beat0_time_ns, 123);
    }

    #[test]
    fn local_summary_score_save_decision_filters_unsaved_summaries() {
        assert_eq!(
            local_summary_score_save_decision(LocalSummaryScoreSaveInput {
                chart_hash: " ",
                disqualified: false,
                score_valid: true,
            }),
            LocalSummaryScoreSaveDecision::SkipEmptyChartHash
        );
        assert_eq!(
            local_summary_score_save_decision(LocalSummaryScoreSaveInput {
                chart_hash: "abc",
                disqualified: true,
                score_valid: true,
            }),
            LocalSummaryScoreSaveDecision::SkipDisqualified
        );
        assert_eq!(
            local_summary_score_save_decision(LocalSummaryScoreSaveInput {
                chart_hash: "abc",
                disqualified: false,
                score_valid: false,
            }),
            LocalSummaryScoreSaveDecision::SkipInvalid
        );
        assert_eq!(
            local_summary_score_save_decision(LocalSummaryScoreSaveInput {
                chart_hash: "abc",
                disqualified: false,
                score_valid: true,
            }),
            LocalSummaryScoreSaveDecision::Save
        );
    }

    #[test]
    fn leaderboard_pane_kind_helpers_match_legacy_names() {
        let hard_ex = LeaderboardPane {
            name: "Hard EX".to_string(),
            entries: Vec::new(),
            is_ex: true,
            disabled: false,
            personalized: true,
            arrowcloud_kind: Some(ArrowCloudPaneKind::HardEx),
        };
        assert!(hard_ex.is_arrowcloud());
        assert!(hard_ex.is_hard_ex());

        let legacy_arrowcloud = LeaderboardPane {
            name: "ArrowCloud".to_string(),
            entries: Vec::new(),
            is_ex: true,
            disabled: false,
            personalized: true,
            arrowcloud_kind: None,
        };
        assert!(legacy_arrowcloud.is_arrowcloud());
        assert!(legacy_arrowcloud.is_hard_ex());
    }

    #[test]
    fn leaderboard_pane_skips_empty_entries() {
        assert!(leaderboard_pane("GrooveStats", Vec::new(), false).is_none());
    }

    #[test]
    fn leaderboard_pane_builds_default_online_metadata() {
        let pane = leaderboard_pane("GrooveStats", vec![entry(9876.0)], true).unwrap();

        assert_eq!(pane.name, "GrooveStats");
        assert_eq!(pane.entries.len(), 1);
        assert_eq!(pane.entries[0].score, 9876.0);
        assert!(pane.is_ex);
        assert!(!pane.disabled);
        assert!(pane.personalized);
        assert_eq!(pane.arrowcloud_kind, None);
    }

    #[test]
    fn arrowcloud_pane_kind_accepts_server_variants() {
        assert_eq!(
            arrowcloud_pane_kind_from_type("HardEX"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("hard-ex"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("HEX"),
            Some(ArrowCloudPaneKind::HardEx)
        );
        assert_eq!(
            arrowcloud_pane_kind_from_type("EX"),
            Some(ArrowCloudPaneKind::Ex)
        );
        assert_eq!(arrowcloud_pane_kind_from_type("event"), None);
    }

    #[test]
    fn arrowcloud_user_context_marks_self_and_rivals() {
        let context = ArrowCloudUserContext {
            self_user_id: Some("self".to_string()),
            rival_user_ids: HashSet::from([String::from("rival")]),
        };
        assert_eq!(arrowcloud_user_id("  self  "), Some("self"));
        assert_eq!(arrowcloud_user_id("   "), None);

        let targets = arrowcloud_target_user_ids(Some(&context));
        assert!(targets.contains("self"));
        assert!(targets.contains("rival"));

        assert_eq!(
            arrowcloud_entry_flags(Some("self"), false, false, Some(&context)),
            (true, false)
        );
        assert_eq!(
            arrowcloud_entry_flags(Some("rival"), false, false, Some(&context)),
            (false, true)
        );
        assert_eq!(
            arrowcloud_entry_flags(None, true, true, Some(&context)),
            (true, true)
        );
    }

    #[test]
    fn arrowcloud_leaderboard_entry_normalizes_percent_score() {
        let entry = arrowcloud_leaderboard_entry(
            12,
            "YOU".to_string(),
            98.31,
            "2026-05-03T19:10:17.504Z".to_string(),
            true,
            false,
        );

        assert_eq!(entry.rank, 12);
        assert_eq!(entry.name, "YOU");
        assert_eq!(entry.score, 9831.0);
        assert!(entry.is_self);
        assert!(!entry.is_rival);
        assert!(!entry.is_fail);

        let invalid = arrowcloud_leaderboard_entry(
            1,
            "BAD".to_string(),
            f64::NAN,
            String::new(),
            false,
            true,
        );
        assert_eq!(invalid.score, 0.0);
        assert!(invalid.is_rival);
    }

    #[test]
    fn arrowcloud_hard_ex_pane_helpers_set_stable_metadata() {
        let pane = arrowcloud_hard_ex_leaderboard_pane(vec![entry(9980.0)], true);
        assert_eq!(pane.name, "ArrowCloud");
        assert!(pane.personalized);
        assert!(!pane.is_ex);
        assert!(pane.is_arrowcloud());
        assert!(pane.is_hard_ex());
        assert_eq!(pane.entries.len(), 1);

        let empty = arrowcloud_empty_hard_ex_leaderboard_pane();
        assert!(empty.entries.is_empty());
        assert!(!empty.personalized);
        assert!(empty.is_arrowcloud());
        assert!(empty.is_hard_ex());
    }

    #[test]
    fn submit_retry_policy_rounds_remaining_duration_up() {
        assert_eq!(SUBMIT_RETRY_MAX_ATTEMPTS, 5);
        assert_eq!(submit_retry_delay_secs(1), 2);
        assert_eq!(submit_retry_delay_secs(5), 32);
        assert_eq!(duration_to_ceil_secs(Duration::from_secs(16)), 16);
        assert_eq!(duration_to_ceil_secs(Duration::from_millis(15_001)), 16);
    }

    #[derive(Debug, Clone, PartialEq)]
    struct RetryProbe {
        hash: String,
        side: u8,
        value: u8,
        attempt: u8,
        next_retry_at: Option<Instant>,
    }

    fn retry_probe(hash: &str, side: u8, value: u8) -> RetryProbe {
        RetryProbe {
            hash: hash.to_string(),
            side,
            value,
            attempt: 0,
            next_retry_at: None,
        }
    }

    #[test]
    fn submit_retry_state_upserts_caps_and_resets_by_key() {
        let mut state = SubmitRetryState::<RetryProbe>::default();

        state.upsert_by_key(0, retry_probe("one", 0, 1), |entry| entry.hash.as_str(), 2);
        state.upsert_by_key(0, retry_probe("ONE", 0, 2), |entry| entry.hash.as_str(), 2);
        assert_eq!(state.entries(0).len(), 1);
        assert_eq!(
            state
                .get_by_key(0, "one", |entry| entry.hash.as_str())
                .map(|entry| entry.value),
            Some(2)
        );

        state.upsert_by_key(0, retry_probe("two", 0, 3), |entry| entry.hash.as_str(), 2);
        state.upsert_by_key(
            0,
            retry_probe("three", 0, 4),
            |entry| entry.hash.as_str(),
            2,
        );
        assert_eq!(
            state
                .entries(0)
                .iter()
                .map(|entry| entry.hash.as_str())
                .collect::<Vec<_>>(),
            vec!["two", "three"]
        );

        state
            .get_mut_by_key(0, "THREE", |entry| entry.hash.as_str())
            .unwrap()
            .value = 5;
        assert_eq!(
            state.get_by_key(0, "three", |entry| entry.hash.as_str()),
            Some(&RetryProbe {
                hash: "three".to_string(),
                side: 0,
                value: 5,
                attempt: 0,
                next_retry_at: None,
            })
        );

        state.reset_by_key(0, "TWO", |entry| entry.hash.as_str());
        assert_eq!(state.entries(0).len(), 1);
        assert_eq!(
            state.get_by_key(0, "two", |entry| entry.hash.as_str()),
            None
        );
    }

    #[test]
    fn submit_retry_state_tracks_schedule_and_due_retries() {
        let mut state = SubmitRetryState::<RetryProbe>::default();
        let now = Instant::now();
        state.upsert_by_key(0, retry_probe("one", 0, 1), |entry| entry.hash.as_str(), 4);
        state.upsert_by_key(1, retry_probe("two", 1, 2), |entry| entry.hash.as_str(), 4);

        assert!(state.record_failure_by_key(
            0,
            "ONE",
            true,
            SUBMIT_RETRY_MAX_ATTEMPTS,
            now,
            |entry| entry.hash.as_str(),
            |entry| &mut entry.attempt,
            |entry| &mut entry.next_retry_at,
        ));
        assert_eq!(
            state.retry_attempt_by_key(
                0,
                "one",
                |entry| entry.hash.as_str(),
                |entry| entry.attempt
            ),
            Some(1)
        );
        assert_eq!(
            state.remaining_secs_by_key(
                0,
                "one",
                now,
                |entry| entry.hash.as_str(),
                |entry| entry.next_retry_at,
            ),
            Some(2)
        );
        assert!(
            state
                .take_ready_by_key(
                    0,
                    "one",
                    true,
                    now,
                    |entry| entry.hash.as_str(),
                    |entry| &mut entry.next_retry_at,
                )
                .is_none()
        );

        let due_at = now + Duration::from_secs(2);
        assert_eq!(
            state.due_retries(
                due_at,
                |entry| entry.hash.as_str(),
                |entry| entry.side,
                |entry| entry.attempt,
                |entry| entry.next_retry_at,
            ),
            vec![("one".to_string(), 0, 1)]
        );
        assert_eq!(
            state
                .take_ready_by_key(
                    0,
                    "one",
                    true,
                    due_at,
                    |entry| entry.hash.as_str(),
                    |entry| &mut entry.next_retry_at,
                )
                .map(|entry| entry.hash),
            Some("one".to_string())
        );
        assert_eq!(
            state.remaining_secs_by_key(
                0,
                "one",
                due_at,
                |entry| entry.hash.as_str(),
                |entry| entry.next_retry_at,
            ),
            None
        );

        assert!(state.record_failure_by_key(
            1,
            "two",
            false,
            SUBMIT_RETRY_MAX_ATTEMPTS,
            now,
            |entry| entry.hash.as_str(),
            |entry| &mut entry.attempt,
            |entry| &mut entry.next_retry_at,
        ));
        assert_eq!(
            state.retry_attempt_by_key(
                1,
                "two",
                |entry| entry.hash.as_str(),
                |entry| entry.attempt
            ),
            Some(0)
        );
    }

    #[test]
    fn submit_ui_state_updates_matching_tokens_only() {
        let mut state = SubmitUiState::<GrooveStatsSubmitUiStatus>::default();
        state.set(0, "abc", 11, GrooveStatsSubmitUiStatus::Submitting);

        assert_eq!(
            state.get(0, "ABC"),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );
        assert!(!state.update_if_token(0, "abc", 12, GrooveStatsSubmitUiStatus::Submitted));
        assert_eq!(
            state.get(0, "abc"),
            Some(GrooveStatsSubmitUiStatus::Submitting)
        );
        assert!(state.update_if_token(0, "abc", 11, GrooveStatsSubmitUiStatus::Submitted));
        assert_eq!(
            state.get(0, "abc"),
            Some(GrooveStatsSubmitUiStatus::Submitted)
        );

        state.reset(0, "ABC");
        assert_eq!(state.get(0, "abc"), None);
    }

    #[test]
    fn submit_event_ui_state_tracks_progress_and_banner() {
        let mut state =
            SubmitEventUiState::<Vec<EventProgress>, GrooveStatsSubmitRecordBanner>::default();
        let progress = EventProgress {
            name: "ITL".to_string(),
            current_points: 10,
            ..EventProgress::default()
        };

        state.arm(1, "abc", 7);
        state.update_if_token(
            1,
            "ABC",
            7,
            vec![progress],
            Some(GrooveStatsSubmitRecordBanner::WorldRecord),
        );
        assert_eq!(state.progress(1, "abc").len(), 1);
        assert_eq!(
            state.banner(1, "abc"),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );

        state.arm(1, "abc", 8);
        assert!(state.progress(1, "abc").is_empty());
        assert_eq!(state.banner(1, "abc"), None);
    }

    #[test]
    fn groovestats_submit_status_policy_classifies_retry_and_http() {
        assert!(!GrooveStatsSubmitUiStatus::Submitting.can_retry());
        assert!(!GrooveStatsSubmitUiStatus::Submitted.can_retry());
        assert!(GrooveStatsSubmitUiStatus::TimedOut.can_retry());
        assert!(GrooveStatsSubmitUiStatus::NetworkError.can_retry());
        assert!(GrooveStatsSubmitUiStatus::ServerError { http_status: 500 }.can_retry());
        assert!(
            !GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
            .can_retry()
        );
        assert!(GrooveStatsSubmitUiStatus::TimedOut.is_auto_retryable());
        assert!(!GrooveStatsSubmitUiStatus::NetworkError.is_auto_retryable());

        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(408),
            GrooveStatsSubmitUiStatus::TimedOut
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(503),
            GrooveStatsSubmitUiStatus::ServerError { http_status: 503 }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(401),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(404),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::NotFound,
            }
        );
        assert_eq!(
            GrooveStatsSubmitUiStatus::from_http_status(400),
            GrooveStatsSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );
    }

    #[test]
    fn groovestats_eval_masks_match_old_submit_bits() {
        assert_eq!(
            GS_INVALID_REMOVE_MASK,
            (1u8 << 0)
                | (1u8 << 2)
                | (1u8 << 3)
                | (1u8 << 4)
                | (1u8 << 5)
                | (1u8 << 6)
                | (1u8 << 7)
        );
        assert_eq!(GS_INVALID_INSERT_MASK, u8::MAX);
        assert_eq!(GS_INVALID_HOLDS_MASK, 1u8 << 3);
    }

    #[test]
    fn groovestats_reason_lines_keep_legacy_order_and_details() {
        let mut checks = [true; GROOVESTATS_REASON_COUNT];
        checks[1] = false;
        checks[6] = false;
        checks[7] = false;
        let reasons =
            groovestats_reason_lines(&checks, &[String::from("- Custom Fantastic window (18ms)")]);

        assert_eq!(
            reasons,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "Metrics or preferences are incorrect.",
                "- Custom Fantastic window (18ms)",
                "Music rate must be between 1.0x and 3.0x.",
            ]
        );
    }

    #[test]
    fn groovestats_eval_state_from_parts_accepts_valid_play() {
        let state = groovestats_eval_state_from_parts(GrooveStatsEvalInput {
            chart_type: "dance-single",
            music_rate: 1.5,
            remove_mask: 1u8 << 1,
            insert_mask: 0,
            holds_mask: 0,
            fail_type_ok: true,
            autoplay_used: false,
            is_course_mode: false,
            course_submit_allowed: false,
            custom_fantastic_window: false,
            custom_fantastic_window_ms: 10,
        });

        assert!(state.valid);
        assert!(state.reason_lines.is_empty());
        assert!(state.manual_qr_url.is_none());
    }

    #[test]
    fn groovestats_eval_state_from_parts_reports_policy_reasons() {
        let state = groovestats_eval_state_from_parts(GrooveStatsEvalInput {
            chart_type: "dance-solo",
            music_rate: 0.75,
            remove_mask: 0,
            insert_mask: 0,
            holds_mask: 0,
            fail_type_ok: false,
            autoplay_used: true,
            is_course_mode: true,
            course_submit_allowed: false,
            custom_fantastic_window: true,
            custom_fantastic_window_ms: 18,
        });

        assert!(!state.valid);
        assert_eq!(
            state.reason_lines,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "GrooveStats QR is unavailable in course mode.",
                "Metrics or preferences are incorrect.",
                "- Custom Fantastic window (18ms)",
                "Music rate must be between 1.0x and 3.0x.",
                "Fail type must be Immediate or ImmediateContinue.",
                "Autoplay or replay is not allowed.",
            ]
        );
    }

    #[test]
    fn groovestats_gameplay_eval_policy_adds_runtime_reasons() {
        let base = GrooveStatsEvalState {
            valid: true,
            reason_lines: Vec::new(),
            manual_qr_url: None,
        };
        let input = GrooveStatsGameplayEvalInput {
            song_has_lua: false,
            lua_submit_allowed: true,
            song_completed_naturally: true,
            is_failing: false,
            life: 1.0,
            has_fail_time: false,
            course_stage_life_submit_eligible: true,
        };

        let result = groovestats_eval_state_from_gameplay_parts(base.clone(), input);
        assert!(result.state.valid);
        assert!(result.should_set_manual_qr_url);

        let result = groovestats_eval_state_from_gameplay_parts(
            base.clone(),
            GrooveStatsGameplayEvalInput {
                song_has_lua: true,
                lua_submit_allowed: false,
                ..input
            },
        );
        assert!(!result.state.valid);
        assert_eq!(result.state.reason_lines, vec!["simfile relies on lua"]);
        assert!(!result.should_set_manual_qr_url);

        let result = groovestats_eval_state_from_gameplay_parts(
            base.clone(),
            GrooveStatsGameplayEvalInput {
                song_completed_naturally: false,
                ..input
            },
        );
        assert!(!result.state.valid);
        assert_eq!(
            result.state.reason_lines,
            vec!["Only completed stages can be submitted."]
        );

        let result = groovestats_eval_state_from_gameplay_parts(
            base.clone(),
            GrooveStatsGameplayEvalInput {
                is_failing: true,
                ..input
            },
        );
        assert!(!result.state.valid);
        assert_eq!(
            result.state.reason_lines,
            vec!["Only passing scores are submitted."]
        );

        let result = groovestats_eval_state_from_gameplay_parts(
            base,
            GrooveStatsGameplayEvalInput {
                course_stage_life_submit_eligible: false,
                ..input
            },
        );
        assert!(!result.state.valid);
        assert_eq!(
            result.state.reason_lines,
            vec!["Course stage would have failed from normal life."]
        );
    }

    #[test]
    fn itl_eval_state_from_parts_accepts_valid_play() {
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: false,
            used_cmod: false,
            groovestats_valid: true,
            groovestats_reason_lines: &[],
            music_rate: 1.0,
            mines_enabled: true,
            all_timing_windows_enabled: true,
            passed: true,
        });

        assert!(state.active);
        assert!(state.eligible);
        assert!(!state.chart_no_cmod);
        assert!(!state.used_cmod);
        assert!(state.reason_lines.is_empty());
    }

    #[test]
    fn itl_eval_state_from_parts_reports_policy_reasons() {
        let gs_reasons = vec!["GrooveStats does not support dance-solo charts.".to_string()];
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: true,
            used_cmod: true,
            groovestats_valid: false,
            groovestats_reason_lines: gs_reasons.as_slice(),
            music_rate: 1.5,
            mines_enabled: false,
            all_timing_windows_enabled: false,
            passed: false,
        });

        assert!(state.active);
        assert!(!state.eligible);
        assert_eq!(
            state.reason_lines,
            vec![
                "GrooveStats does not support dance-solo charts.",
                "ITL requires 1.00x music rate.",
                "ITL requires mines to be enabled.",
                "ITL requires all timing windows to be enabled.",
                "ITL only saves passing scores.",
                "This ITL chart does not allow CMod.",
            ]
        );
    }

    #[test]
    fn itl_eval_state_from_parts_falls_back_for_missing_gs_reason() {
        let state = itl_eval_state_from_parts(ItlEvalInput {
            chart_no_cmod: false,
            used_cmod: false,
            groovestats_valid: false,
            groovestats_reason_lines: &[],
            music_rate: 1.0,
            mines_enabled: true,
            all_timing_windows_enabled: true,
            passed: true,
        });

        assert_eq!(
            state.reason_lines,
            vec!["Score is not valid for GrooveStats."]
        );
    }

    #[test]
    fn save_itl_chart_result_adds_new_entry_and_progress() {
        let mut data = ItlFileData::default();
        let result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: "/Songs/ITL Online 2026/Example",
                chart_hash: "deadbeef",
                chart_name: "7500 (P) + 12000 (S)",
                chart_type: "dance-double",
                event_name: "ITL Online 2026",
                judgments: ItlJudgments {
                    w0: 10,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex_percent: 100.0,
                used_cmod: false,
                chart_no_cmod: true,
                date: "2026-06-23".to_string(),
            },
        );

        assert!(result.needs_write);
        assert_eq!(data.path_map["/Songs/ITL Online 2026/Example"], "deadbeef");
        let entry = &data.hash_map["deadbeef"];
        assert_eq!(entry.ex, 10_000);
        assert_eq!(entry.points, 19_500);
        assert_eq!(entry.passing_points, 7500);
        assert_eq!(entry.max_scoring_points, 12000);
        assert_eq!(entry.passes, 1);
        assert_eq!(entry.steps_type, "double");
        assert!(entry.no_cmod);
        assert_eq!(result.progress.name, "ITL Online 2026");
        assert!(result.progress.is_doubles);
        assert_eq!(result.progress.current_points, 19_500);
        assert_eq!(result.progress.score_delta_hundredths, 10_000);
        assert_eq!(result.progress.clear_type_before, Some(0));
        assert_eq!(result.progress.clear_type_after, Some(5));
        assert!(!result.progress.overlay_pages.is_empty());
    }

    #[test]
    fn save_itl_chart_result_updates_passes_and_better_tied_judgments() {
        let mut data = ItlFileData::default();
        data.path_map.insert(
            "/Songs/ITL Online 2026/Example".to_string(),
            "deadbeef".to_string(),
        );
        data.hash_map.insert(
            "deadbeef".to_string(),
            ItlHashEntry {
                judgments: ItlJudgments {
                    w0: 9,
                    w1: 1,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex: 10_000,
                clear_type: 4,
                points: 19_500,
                passing_points: 7500,
                max_scoring_points: 12000,
                max_points: 19_500,
                passes: 1,
                steps_type: "single".to_string(),
                ..ItlHashEntry::default()
            },
        );

        let result = save_itl_chart_result(
            &mut data,
            ItlChartSaveInput {
                song_dir: "/Songs/ITL Online 2026/Example",
                chart_hash: "deadbeef",
                chart_name: "",
                chart_type: "dance-single",
                event_name: "",
                judgments: ItlJudgments {
                    w0: 10,
                    total_steps: 10,
                    ..ItlJudgments::default()
                },
                ex_percent: 100.0,
                used_cmod: true,
                chart_no_cmod: false,
                date: "2026-06-24".to_string(),
            },
        );

        let entry = &data.hash_map["deadbeef"];
        assert!(result.needs_write);
        assert_eq!(entry.passes, 2);
        assert_eq!(entry.judgments.w0, 10);
        assert!(entry.used_cmod);
        assert_eq!(entry.date, "2026-06-24");
        assert_eq!(result.progress.name, "ITL Online 2026");
        assert_eq!(result.progress.total_passes, 2);
        assert_eq!(result.progress.point_delta, 0);
        assert_eq!(result.progress.score_delta_hundredths, 0);
    }

    #[test]
    fn save_itl_gameplay_players_writes_eligible_scores() {
        let mut written = Vec::new();
        let mut cached = Vec::new();
        let mut skips = Vec::new();
        let players = vec![ItlGameplaySavePlayer {
            player_idx: 1,
            profile_id: "profile-a".to_string(),
            song_dir: Some("/Songs/ITL Online 2026/Example".to_string()),
            event_name: Some("ITL Online 2026".to_string()),
            chart_hash: "deadbeef".to_string(),
            chart_name: "7500 (P) + 12000 (S)".to_string(),
            chart_type: "dance-single".to_string(),
            subtitle: String::new(),
            used_cmod: false,
            groovestats_valid: true,
            groovestats_reason_lines: Vec::new(),
            music_rate: 1.0,
            remove_mask: 0,
            disabled_windows: [false; 5],
            passed: true,
            judgments: ItlJudgments {
                w0: 10,
                total_steps: 10,
                ..ItlJudgments::default()
            },
            ex_percent: 100.0,
            date: "2026-06-25".to_string(),
        }];

        let progress = save_itl_gameplay_players(
            players,
            |_| ItlFileData::default(),
            |profile_id, data| written.push((profile_id.to_string(), data.clone())),
            |profile_id, data| cached.push((profile_id.to_string(), data)),
            |skip| skips.push((skip.player_idx, skip.reason_lines.join("; "))),
        );

        assert!(skips.is_empty());
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0].player_idx, 1);
        assert_eq!(progress[0].progress.current_points, 19_500);
        assert_eq!(written.len(), 1);
        assert_eq!(cached.len(), 1);
        assert_eq!(written[0].0, "profile-a");
        assert_eq!(cached[0].1.hash_map["deadbeef"].ex, 10_000);
    }

    #[test]
    fn arrowcloud_submit_status_policy_matches_shared_retry_rules() {
        assert!(!ArrowCloudSubmitUiStatus::Submitting.can_retry());
        assert!(!ArrowCloudSubmitUiStatus::Submitted.can_retry());
        assert!(ArrowCloudSubmitUiStatus::TimedOut.can_retry());
        assert!(ArrowCloudSubmitUiStatus::NetworkError.can_retry());
        assert!(ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }.can_retry());
        assert!(
            !ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
            .can_retry()
        );
        assert!(ArrowCloudSubmitUiStatus::TimedOut.is_auto_retryable());
        assert!(!ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }.is_auto_retryable());

        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(504),
            ArrowCloudSubmitUiStatus::TimedOut
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(500),
            ArrowCloudSubmitUiStatus::ServerError { http_status: 500 }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(403),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::Unauthorized,
            }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(404),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::NotFound,
            }
        );
        assert_eq!(
            ArrowCloudSubmitUiStatus::from_http_status(418),
            ArrowCloudSubmitUiStatus::Rejected {
                reason: RejectReason::InvalidScore,
            }
        );
    }

    #[test]
    fn groovestats_submit_banner_prefers_selected_world_record() {
        assert_eq!(
            groovestats_submit_record_banner("improved", false, Some(1), Some(4), true),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
        assert_eq!(
            groovestats_submit_record_banner("score-added", true, Some(2), Some(1), true),
            Some(GrooveStatsSubmitRecordBanner::WorldRecordEx)
        );
    }

    #[test]
    fn groovestats_submit_banner_falls_back_to_personal_best() {
        assert_eq!(
            groovestats_submit_record_banner("improved", true, Some(1), None, true),
            Some(GrooveStatsSubmitRecordBanner::PersonalBest)
        );
        assert_eq!(
            groovestats_submit_record_banner("improved", true, Some(1), None, false),
            Some(GrooveStatsSubmitRecordBanner::WorldRecord)
        );
    }

    #[test]
    fn groovestats_submit_banner_ignores_non_improving_results() {
        assert_eq!(
            groovestats_submit_record_banner(
                "score-already-submitted",
                false,
                Some(1),
                None,
                false
            ),
            None
        );
    }

    #[test]
    fn reject_reason_labels_match_ui_copy() {
        assert_eq!(RejectReason::InvalidScore.label(), "Invalid Score");
        assert_eq!(RejectReason::Unauthorized.label(), "Unauthorized");
        assert_eq!(RejectReason::NotFound.label(), "Unknown Chart");
    }

    #[test]
    fn arrowcloud_leaderboard_id_round_trips() {
        for v in ArrowCloudLeaderboard::ALL_GLOBAL {
            assert_eq!(ArrowCloudLeaderboard::from_id(v.id()), Some(v));
        }
        assert_eq!(ArrowCloudLeaderboard::from_id(0), None);
        assert_eq!(ArrowCloudLeaderboard::from_id(9), None);
    }

    #[test]
    fn arrowcloud_leaderboard_serializes_as_integer() {
        let json = serde_json::to_string(&ArrowCloudLeaderboard::Itg).unwrap();
        assert_eq!(json, "3");
        let json = serde_json::to_string(&ArrowCloudLeaderboard::ALL_GLOBAL).unwrap();
        assert_eq!(json, "[4,2,3]");
    }

    #[test]
    fn arrowcloud_server_grade_round_trips_through_str() {
        for grade in [
            ArrowCloudServerGrade::Sex,
            ArrowCloudServerGrade::Quint,
            ArrowCloudServerGrade::Quad,
            ArrowCloudServerGrade::Tristar,
            ArrowCloudServerGrade::SPlus,
            ArrowCloudServerGrade::SMinus,
            ArrowCloudServerGrade::Failed,
        ] {
            assert_eq!(
                ArrowCloudServerGrade::from_server_str(grade.as_str()),
                Some(grade)
            );
        }
    }

    #[test]
    fn arrowcloud_score_bincode_round_trip_preserves_date() {
        let original = ArrowCloudScore {
            score_percent: 0.9989,
            server_grade: Some(ArrowCloudServerGrade::Tristar),
            played_at: Some(Utc.timestamp_millis_opt(1_777_835_417_504).unwrap()),
            play_id: Some(12345),
            is_fail: false,
        };
        let cfg = bincode::config::standard();
        let bytes = bincode::encode_to_vec(original, cfg).unwrap();
        let (decoded, _): (ArrowCloudScore, _) = bincode::decode_from_slice(&bytes, cfg).unwrap();
        assert_eq!(decoded, original);
    }

    fn ac_score(percent_0_1: f64, is_fail: bool) -> ArrowCloudScore {
        ArrowCloudScore {
            score_percent: percent_0_1,
            server_grade: None,
            played_at: None,
            play_id: None,
            is_fail,
        }
    }

    #[test]
    fn arrowcloud_retrieve_fields_preserve_native_metadata() {
        let score = arrowcloud_score_from_retrieve_fields(
            Some(99.89),
            Some("Tristar"),
            Some("2026-05-03T19:10:17.504Z"),
            Some(12345),
            false,
        )
        .expect("score should decode");

        assert!((score.score_percent - 0.9989).abs() < 1e-6);
        assert_eq!(score.server_grade, Some(ArrowCloudServerGrade::Tristar));
        assert_eq!(score.play_id, Some(12345));
        assert_eq!(
            score
                .played_at
                .expect("played_at parsed")
                .timestamp_millis(),
            1_777_835_417_504
        );
    }

    #[test]
    fn arrowcloud_retrieve_fields_drop_missing_score_and_bad_metadata() {
        assert!(
            arrowcloud_score_from_retrieve_fields(None, Some("Tristar"), None, None, false)
                .is_none()
        );

        let score = arrowcloud_score_from_retrieve_fields(
            Some(125.0),
            Some("Mythic"),
            Some("not-a-date"),
            None,
            true,
        )
        .expect("score should decode");

        assert_eq!(score.score_percent, 1.0);
        assert_eq!(score.server_grade, None);
        assert_eq!(score.played_at, None);
        assert!(score.is_fail);
    }

    #[test]
    fn arrowcloud_submit_percent_clamps_and_rejects_nan() {
        let submitted_at = Utc.timestamp_millis_opt(1_777_835_417_504).unwrap();
        let score = arrowcloud_score_from_submit_percent(120.0, false, submitted_at)
            .expect("finite submit percent should decode");

        assert_eq!(score.score_percent, 1.0);
        assert_eq!(score.server_grade, None);
        assert_eq!(score.played_at, Some(submitted_at));
        assert!(arrowcloud_score_from_submit_percent(f64::NAN, false, submitted_at).is_none());
    }

    #[test]
    fn arrowcloud_leaderboard_slot_assignment_ignores_unknown_ids() {
        let mut scores = ArrowCloudScores::default();

        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::HardEx.id(),
            ac_score(0.9951, false)
        ));
        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::Ex.id(),
            ac_score(0.9810, false)
        ));
        assert!(set_arrowcloud_score_for_leaderboard(
            &mut scores,
            ArrowCloudLeaderboard::Itg.id(),
            ac_score(0.9989, false)
        ));
        assert!(!set_arrowcloud_score_for_leaderboard(
            &mut scores,
            9,
            ac_score(0.9500, false)
        ));

        assert_eq!(scores.itg.unwrap().score_percent, 0.9989);
        assert_eq!(scores.ex.unwrap().score_percent, 0.9810);
        assert_eq!(scores.hard_ex.unwrap().score_percent, 0.9951);
    }

    #[test]
    fn arrowcloud_score_slot_merge_keeps_best_non_failed_score() {
        let mut slot = Some(ac_score(0.85, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.95, true)));
        assert!(!slot.as_ref().unwrap().is_fail);

        let mut slot = Some(ac_score(0.85, true));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.50, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.50);
        assert!(!slot.as_ref().unwrap().is_fail);

        let mut slot = Some(ac_score(0.90, false));
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.92, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.92);
        merge_arrowcloud_score_slot(&mut slot, Some(ac_score(0.91, false)));
        assert_eq!(slot.as_ref().unwrap().score_percent, 0.92);
    }

    fn ac_judgment(grade: judgment::JudgeGrade, offset_ms: f32) -> judgment::Judgment {
        judgment::Judgment {
            time_error_ms: offset_ms,
            time_error_music_ns: judgment::judgment_time_error_music_ns_from_ms(offset_ms, 1.0),
            grade,
            window: match grade {
                judgment::JudgeGrade::Fantastic => Some(judgment::TimingWindow::W0),
                judgment::JudgeGrade::Excellent => Some(judgment::TimingWindow::W2),
                judgment::JudgeGrade::Great => Some(judgment::TimingWindow::W3),
                judgment::JudgeGrade::Decent => Some(judgment::TimingWindow::W4),
                judgment::JudgeGrade::WayOff => Some(judgment::TimingWindow::W5),
                judgment::JudgeGrade::Miss => None,
            },
            miss_because_held: false,
        }
    }

    fn ac_note(
        row_index: usize,
        note_type: NoteType,
        result: Option<judgment::Judgment>,
        hold_result: Option<HoldResult>,
        mine_result: Option<MineResult>,
    ) -> Note {
        Note {
            beat: row_index as f32,
            quantization_idx: 0,
            column: 0,
            note_type,
            row_index,
            result,
            early_result: None,
            hold: hold_result.map(|result| deadsync_rules::note::HoldData {
                end_row_index: row_index,
                end_beat: row_index as f32,
                result: Some(result),
                life: 1.0,
                let_go_started_at: None,
                let_go_starting_life: 1.0,
                last_held_row_index: row_index,
                last_held_beat: row_index as f32,
            }),
            mine_result,
            is_fake: false,
            can_be_judged: true,
        }
    }

    #[test]
    fn arrowcloud_submit_stats_caps_failed_runs_at_fail_time() {
        let ns = deadsync_core::song_time::song_time_ns_from_seconds;
        let notes = vec![
            ac_note(
                0,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 5.0)),
                None,
                None,
            ),
            ac_note(
                1,
                NoteType::Hold,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 8.0)),
                Some(HoldResult::Held),
                None,
            ),
            ac_note(
                2,
                NoteType::Roll,
                Some(ac_judgment(judgment::JudgeGrade::Fantastic, 10.0)),
                Some(HoldResult::Held),
                None,
            ),
            ac_note(3, NoteType::Mine, None, None, Some(MineResult::Hit)),
            ac_note(
                4,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Miss, 180.0)),
                None,
                None,
            ),
            ac_note(
                5,
                NoteType::Tap,
                Some(ac_judgment(judgment::JudgeGrade::Great, 55.0)),
                None,
                None,
            ),
            ac_note(6, NoteType::Mine, None, None, Some(MineResult::Hit)),
        ];
        let note_times = vec![
            ns(1.0),
            ns(1.2),
            ns(1.4),
            ns(1.5),
            ns(2.0),
            ns(3.0),
            ns(3.5),
        ];
        let hold_end_times = vec![None, Some(ns(1.8)), Some(ns(2.4)), None, None, None, None];

        let stats = arrowcloud_submit_stats_from_results(
            &notes,
            &note_times,
            &hold_end_times,
            Some(ns(2.0)),
        );

        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Fantastic)],
            3
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Miss)],
            1
        );
        assert_eq!(
            stats.judgment_counts[judgment::judge_grade_ix(judgment::JudgeGrade::Great)],
            0
        );
        assert_eq!(stats.window_counts.w0, 3);
        assert_eq!(stats.window_counts.miss, 1);
        assert_eq!(stats.holds_held, 1);
        assert_eq!(stats.rolls_held, 0);
        assert_eq!(stats.mines_hit, 1);
    }

    #[test]
    fn lua_submit_allowlist_is_disabled() {
        assert!(!lua_chart_submit_allowed("d5bd4dd7224f68ff"));
        assert!(!lua_chart_submit_allowed(" D5BD4DD7224F68FF "));
        assert!(!lua_chart_submit_allowed("deadbeefcafebabe"));
        assert!(lua_submit_allowed(false, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "deadbeefcafebabe"));
        assert!(!lua_submit_allowed(true, "d5bd4dd7224f68ff"));
    }

    #[test]
    fn groovestats_comment_counts_ignore_ds_prefix() {
        let counts = parse_gs_comment_counts("[DS], 15e, 2g, 2m");
        assert_eq!(counts.e, 15);
        assert_eq!(counts.g, 2);
        assert_eq!(counts.m, 2);
    }

    #[test]
    fn groovestats_comment_counts_accept_mixed_case_suffixes() {
        let counts = parse_gs_comment_counts("1W, 2E, 3G, 4D, 5wO, 6M");
        assert_eq!(
            counts,
            GsCommentCounts {
                w: 1,
                e: 2,
                g: 3,
                d: 4,
                wo: 5,
                m: 6,
            }
        );
        for comment in ["7wo", "7Wo", "7wO", "7WO"] {
            assert_eq!(parse_gs_comment_counts(comment).wo, 7);
        }
    }

    #[test]
    fn groovestats_comment_counts_ignore_unknown_suffixes() {
        assert_eq!(
            parse_gs_comment_counts("1x, 2miss, 3é, 4, 0W"),
            GsCommentCounts::default()
        );
    }

    #[test]
    fn groovestats_comment_ex_percent_accepts_ds_formats() {
        assert_eq!(
            parse_gs_comment_ex_percent("[DS], FA+, 99.78EX"),
            Some(99.78)
        );
        assert_eq!(
            parse_gs_comment_ex_percent("[DS], FA+, 99.78% EX, C650"),
            Some(99.78)
        );
        assert_eq!(parse_gs_comment_ex_percent("[DS], 3 excellents"), None);
    }

    #[test]
    fn groovestats_score_10000_from_counts_uses_itg_score_math() {
        let mut counts = [0u32; deadsync_rules::judgment::JUDGE_GRADE_COUNT];
        counts[deadsync_rules::judgment::judge_grade_ix(
            deadsync_rules::judgment::JudgeGrade::Fantastic,
        )] = 100;
        assert_eq!(
            groovestats_score_10000_from_counts(&counts, 2, 1, 0, 515),
            10_000
        );

        counts[deadsync_rules::judgment::judge_grade_ix(
            deadsync_rules::judgment::JudgeGrade::Miss,
        )] = 1;
        assert_eq!(
            groovestats_score_10000_from_counts(&counts, 2, 1, 0, 520),
            9_673
        );
        assert_eq!(groovestats_score_10000_from_counts(&counts, 0, 0, 0, 0), 0);
    }

    #[test]
    fn groovestats_rate_and_cmod_helpers_normalize_payload_values() {
        assert_eq!(groovestats_rate_hundredths(1.5), 150);
        assert_eq!(groovestats_rate_hundredths(0.0), 100);
        assert_eq!(groovestats_rate_hundredths(f32::NAN), 100);
        assert!(groovestats_used_cmod(ScrollSpeedSetting::CMod(650.0)));
        assert!(!groovestats_used_cmod(ScrollSpeedSetting::MMod(650.0)));
    }

    #[test]
    fn groovestats_ex_evidence_prefers_scoreboard() {
        let comment = Some("[DS], FA+, 100.00EX");
        assert_eq!(
            GsExEvidence::from_sources(None, comment).is_quint(),
            Some(true)
        );
        assert_eq!(
            GsExEvidence::from_sources(Some(9_978.0), comment).is_quint(),
            Some(false)
        );
        assert!(GsExEvidence::from_sources(Some(9_978.0), comment).proves_nonquint());
    }

    #[test]
    fn groovestats_lamp_judge_count_ignores_ds_prefix() {
        assert_eq!(gs_lamp_judge_count(Some(1), Some("[DS], 4w")), Some(4));
        assert_eq!(gs_lamp_judge_count(Some(2), Some("[DS], 5e")), Some(5));
        assert_eq!(gs_lamp_judge_count(Some(3), Some("[DS], 2g")), Some(2));
        assert_eq!(gs_lamp_judge_count(Some(1), Some("[DS], 10w")), None);
    }

    #[test]
    fn groovestats_lamp_index_uses_ex_evidence_for_perfect_scores() {
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                10_000.0,
                Some("[DS], FA+, 99.78EX"),
                GsExEvidence::from_sources(Some(10_000.0), Some("[DS], FA+, 99.78EX")),
                None,
            ),
            Some(0)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                10_000.0,
                Some("[DS], FA+, 100.00EX, 3w"),
                GsExEvidence::from_sources(Some(9_978.0), Some("[DS], FA+, 100.00EX, 3w")),
                None,
            ),
            Some(1)
        );
    }

    #[test]
    fn groovestats_lamp_index_reconstructs_full_combo_lamps() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });

        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_960.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                stats
            ),
            Some(2)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_940.0,
                Some("[DS], 1g"),
                GsExEvidence::default(),
                stats
            ),
            Some(3)
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_900.0,
                Some("[DS], 1d"),
                GsExEvidence::default(),
                stats
            ),
            Some(4)
        );
    }

    #[test]
    fn groovestats_lamp_index_rejects_missing_stats_or_dp_mismatch() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });

        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_960.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                None
            ),
            None
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_950.0,
                Some("[DS], 2e"),
                GsExEvidence::default(),
                stats
            ),
            None
        );
        assert_eq!(
            gs_lamp_index_from_chart_stats(
                9_000.0,
                Some("[DS], 1m"),
                GsExEvidence::default(),
                stats
            ),
            None
        );
    }

    #[test]
    fn cached_gs_score_from_lamp_applies_grade_and_judge_count() {
        let quad = cached_gs_score_from_lamp(10_000.0, false, Some(1), Some("[DS], 4w"));
        assert_eq!(quad.grade, Grade::Tier01);
        assert_eq!(quad.lamp_index, Some(1));
        assert_eq!(quad.lamp_judge_count, Some(4));

        let quint = cached_gs_score_from_lamp(10_000.0, false, Some(0), Some("[DS], FA+"));
        assert_eq!(quint.grade, Grade::Quint);
        assert_eq!(quint.lamp_index, Some(0));
        assert_eq!(quint.lamp_judge_count, None);
    }

    #[test]
    fn cached_gs_score_from_chart_stats_reconstructs_lamp_before_caching() {
        let stats = Some(GsLampChartStats {
            total_steps: 100,
            holds: 0,
            rolls: 0,
        });
        let score = cached_gs_score_from_chart_stats(
            9_920.0,
            false,
            Some("[DS], 4e"),
            GsExEvidence::default(),
            stats,
        );
        assert_eq!(score.grade, Grade::Tier02);
        assert_eq!(score.lamp_index, Some(2));
        assert_eq!(score.lamp_judge_count, Some(4));

        let failed = cached_gs_score_from_chart_stats(
            9_920.0,
            true,
            Some("[DS], 4e"),
            GsExEvidence::default(),
            stats,
        );
        assert_eq!(failed.grade, Grade::Failed);
        assert_eq!(failed.score_percent, 0.992);
        assert_eq!(failed.lamp_index, None);
        assert_eq!(failed.lamp_judge_count, None);
    }

    #[test]
    fn imported_gs_scores_use_ex_evidence_and_comments() {
        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 100.00EX".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(None, Some("[DS], FA+, 100.00EX")),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Quint);
        assert_eq!(score.lamp_index, Some(0));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(
                    Some(9_978.0),
                    Some("[DS], FA+, 100.00EX, C875"),
                ),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+, 99.71EX, 3w".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::from_sources(None, Some("[DS], FA+, 99.71EX, 3w")),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
        assert_eq!(score.lamp_judge_count, Some(3));

        let score = cached_score_from_imported_player_score(
            ImportedPlayerScore {
                score_10000: 10_000.0,
                comments: Some("[DS], FA+".to_string()),
                is_fail: false,
                ex_evidence: GsExEvidence::default(),
            },
            None,
        );
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
    }

    #[test]
    fn cached_gs_failed_scores_clear_lamp_fields() {
        let failed = cached_gs_score_from_lamp(9_123.0, true, Some(1), Some("[DS], 4w"));
        assert_eq!(failed.grade, Grade::Failed);
        assert_eq!(failed.lamp_index, None);
        assert_eq!(failed.lamp_judge_count, None);
        assert_eq!(cached_missing_gs_score(), cached_failed_gs_score(0.0));
    }

    #[test]
    fn merge_local_fail_marks_matching_imported_score() {
        let imported = ImportedPlayerScore {
            score_10000: 1_358.0,
            comments: None,
            is_fail: false,
            ex_evidence: GsExEvidence::default(),
        };
        let matching_fail = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        assert!(merge_local_fail(imported.clone(), Some(matching_fail)).is_fail);

        let passed = CachedScore {
            grade: Grade::Tier17,
            ..matching_fail
        };
        assert!(!merge_local_fail(imported.clone(), Some(passed)).is_fail);

        let different_fail = CachedScore {
            score_percent: 0.2,
            ..matching_fail
        };
        assert!(!merge_local_fail(imported, Some(different_fail)).is_fail);
    }

    #[test]
    fn leaderboard_import_cache_result_skips_missing_self_row() {
        let local = cached_score(Grade::Tier01, 0.9934, Some(1), Some(2));
        assert_eq!(
            cached_score_from_leaderboard_import(None, Some(local), None),
            None
        );
    }

    #[test]
    fn leaderboard_import_cache_result_applies_matching_local_fail() {
        let imported = ImportedPlayerScore {
            score_10000: 1_358.0,
            comments: None,
            is_fail: false,
            ex_evidence: GsExEvidence::default(),
        };
        let local_fail = CachedScore {
            grade: Grade::Failed,
            score_percent: 0.1358,
            lamp_index: None,
            lamp_judge_count: None,
        };
        let cached = cached_score_from_leaderboard_import(Some(imported), Some(local_fail), None)
            .expect("leaderboard score should cache");
        assert_eq!(cached.score.grade, Grade::Failed);
        assert!(!cached.score_proves_nonquint_ex);
    }

    #[test]
    fn score_file_shard_uses_two_hash_chars() {
        assert_eq!(score_file_shard("abcdef"), "ab");
        assert_eq!(score_file_shard("a"), "00");
        assert_eq!(score_file_shard(""), "00");
    }

    #[test]
    fn score_file_name_parses_hash_and_timestamp() {
        assert_eq!(
            parse_score_file_name("dead-beef-1234.bin"),
            Some(("dead-beef", 1234))
        );
        assert_eq!(parse_score_file_name("deadbeef.bin"), None);
        assert_eq!(parse_score_file_name("-1234.bin"), None);
        assert_eq!(parse_score_file_name("deadbeef-abc.bin"), None);
        assert_eq!(parse_score_file_name("deadbeef-1234.txt"), None);
    }

    #[test]
    fn gameplay_pass_fail_helpers_use_completion_and_life() {
        assert!(gameplay_run_passed(true, false, 1.0, false));
        assert!(!gameplay_run_passed(false, false, 1.0, false));
        assert!(!gameplay_run_passed(true, true, 1.0, false));
        assert!(!gameplay_run_passed(true, false, 1.0, true));
        assert!(!gameplay_run_passed(true, false, 0.0, false));
        assert!(!gameplay_run_failed(false, false));
        assert!(gameplay_run_failed(true, false));
        assert!(gameplay_run_failed(false, true));
    }

    #[test]
    fn arrowcloud_autosubmit_session_policy_skips_global_blockers() {
        assert_eq!(
            arrowcloud_autosubmit_session_decision(ArrowCloudAutosubmitSessionInput {
                enabled: false,
                player_count: 1,
                autoplay_used: false,
                is_course_stage: false,
                autosubmit_course_scores_individually: false,
            }),
            ArrowCloudAutosubmitSessionDecision::Skip { log: None }
        );
        assert_eq!(
            arrowcloud_autosubmit_session_decision(ArrowCloudAutosubmitSessionInput {
                enabled: true,
                player_count: 1,
                autoplay_used: true,
                is_course_stage: false,
                autosubmit_course_scores_individually: false,
            }),
            ArrowCloudAutosubmitSessionDecision::Skip {
                log: Some(ArrowCloudAutosubmitLog::debug("autoplay/replay was used"))
            }
        );
        assert_eq!(
            arrowcloud_autosubmit_session_decision(ArrowCloudAutosubmitSessionInput {
                enabled: true,
                player_count: 1,
                autoplay_used: false,
                is_course_stage: true,
                autosubmit_course_scores_individually: false,
            }),
            ArrowCloudAutosubmitSessionDecision::Skip {
                log: Some(ArrowCloudAutosubmitLog::debug(
                    "course per-song autosubmit is disabled"
                ))
            }
        );
    }

    #[test]
    fn arrowcloud_autosubmit_player_policy_preserves_submit_gates() {
        let base = ArrowCloudAutosubmitPlayerInput {
            song_has_lua: false,
            lua_submit_allowed: true,
            song_completed_naturally: true,
            is_failing: false,
            life: 1.0,
            has_fail_time: false,
            submit_fails_enabled: false,
            api_key_present: true,
            course_stage_life_submit_eligible: true,
        };
        assert_eq!(
            arrowcloud_autosubmit_player_decision(base).action,
            ArrowCloudAutosubmitPlayerAction::BuildPayload
        );
        assert_eq!(
            arrowcloud_autosubmit_player_decision(ArrowCloudAutosubmitPlayerInput {
                song_has_lua: true,
                lua_submit_allowed: false,
                ..base
            })
            .action,
            ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::debug("simfile relies on lua"))
            }
        );
        assert_eq!(
            arrowcloud_autosubmit_player_decision(ArrowCloudAutosubmitPlayerInput {
                api_key_present: false,
                ..base
            })
            .action,
            ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::warn("profile is missing API key"))
            }
        );
        let failed = arrowcloud_autosubmit_player_decision(ArrowCloudAutosubmitPlayerInput {
            song_completed_naturally: false,
            is_failing: true,
            submit_fails_enabled: true,
            api_key_present: false,
            ..base
        });
        assert!(failed.failed);
        assert!(failed.allow_failed_submit);
        assert_eq!(
            failed.action,
            ArrowCloudAutosubmitPlayerAction::Skip {
                log: Some(ArrowCloudAutosubmitLog::warn("profile is missing API key"))
            }
        );
    }

    #[test]
    fn arrowcloud_autosubmit_payload_policy_keeps_failed_skip_after_payload() {
        assert_eq!(
            arrowcloud_autosubmit_after_payload_decision(true, false),
            Some(ArrowCloudAutosubmitLog::debug(
                "failed-stage submits are disabled"
            ))
        );
        assert_eq!(
            arrowcloud_autosubmit_after_payload_decision(true, true),
            None
        );
        assert_eq!(
            arrowcloud_autosubmit_after_payload_decision(false, false),
            None
        );
    }

    #[test]
    fn groovestats_autosubmit_session_policy_skips_global_blockers() {
        assert_eq!(
            groovestats_autosubmit_session_decision(GrooveStatsAutosubmitSessionInput {
                enabled: false,
                player_count: 1,
                autoplay_used: false,
                is_course_stage: false,
                autosubmit_course_scores_individually: false,
            }),
            GrooveStatsAutosubmitSessionDecision::Skip { log: None }
        );
        assert_eq!(
            groovestats_autosubmit_session_decision(GrooveStatsAutosubmitSessionInput {
                enabled: true,
                player_count: 1,
                autoplay_used: true,
                is_course_stage: false,
                autosubmit_course_scores_individually: false,
            }),
            GrooveStatsAutosubmitSessionDecision::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug("autoplay/replay was used"))
            }
        );
        assert_eq!(
            groovestats_autosubmit_session_decision(GrooveStatsAutosubmitSessionInput {
                enabled: true,
                player_count: 1,
                autoplay_used: false,
                is_course_stage: true,
                autosubmit_course_scores_individually: false,
            }),
            GrooveStatsAutosubmitSessionDecision::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug(
                    "course per-song autosubmit is disabled"
                ))
            }
        );
    }

    #[test]
    fn groovestats_autosubmit_player_policy_preserves_submit_gates() {
        let base = GrooveStatsAutosubmitPlayerInput {
            has_invalid_reason: false,
            is_pad_player: true,
            song_completed_naturally: true,
            is_failing: false,
            life: 1.0,
            has_fail_time: false,
            course_stage_life_submit_eligible: true,
            api_key_present: true,
        };
        assert_eq!(
            groovestats_autosubmit_player_decision(base).action,
            GrooveStatsAutosubmitPlayerAction::BuildPayload
        );
        assert_eq!(
            groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
                has_invalid_reason: true,
                ..base
            })
            .action,
            GrooveStatsAutosubmitPlayerAction::SkipInvalidReason
        );
        assert_eq!(
            groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
                is_pad_player: false,
                ..base
            })
            .action,
            GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::warn(
                    "profile is not marked as a pad player"
                ))
            }
        );
        assert_eq!(
            groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
                song_completed_naturally: false,
                ..base
            })
            .action,
            GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug("stage was not completed"))
            }
        );
        assert_eq!(
            groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
                is_failing: true,
                ..base
            })
            .action,
            GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::debug("stage was not passed"))
            }
        );
        assert_eq!(
            groovestats_autosubmit_player_decision(GrooveStatsAutosubmitPlayerInput {
                api_key_present: false,
                ..base
            })
            .action,
            GrooveStatsAutosubmitPlayerAction::Skip {
                log: Some(GrooveStatsAutosubmitLog::warn("profile is missing API key"))
            }
        );
    }

    #[test]
    fn score_import_endpoint_metadata_matches_backends() {
        assert_eq!(
            ScoreImportEndpoint::GrooveStats.display_name(),
            "GrooveStats"
        );
        assert_eq!(
            ScoreImportEndpoint::BoogieStats.display_name(),
            "BoogieStats"
        );
        assert_eq!(ScoreImportEndpoint::ArrowCloud.display_name(), "ArrowCloud");
        assert!(ScoreImportEndpoint::GrooveStats.requires_username());
        assert!(ScoreImportEndpoint::BoogieStats.requires_username());
        assert!(!ScoreImportEndpoint::ArrowCloud.requires_username());
    }

    #[test]
    fn score_import_endpoint_choices_match_options_order() {
        assert_eq!(
            SCORE_IMPORT_ENDPOINT_CHOICES,
            [
                ScoreImportEndpoint::GrooveStats,
                ScoreImportEndpoint::BoogieStats,
                ScoreImportEndpoint::ArrowCloud,
            ]
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::GrooveStats),
            0
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::BoogieStats),
            1
        );
        assert_eq!(
            score_import_endpoint_choice_index(ScoreImportEndpoint::ArrowCloud),
            2
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(0),
            ScoreImportEndpoint::GrooveStats
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(1),
            ScoreImportEndpoint::BoogieStats
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(2),
            ScoreImportEndpoint::ArrowCloud
        );
        assert_eq!(
            score_import_endpoint_from_choice_index(99),
            ScoreImportEndpoint::GrooveStats
        );
    }

    #[test]
    fn score_import_credential_validation_requires_endpoint_fields() {
        assert_eq!(
            validate_score_import_credentials(ScoreImportEndpoint::GrooveStats, "", "player"),
            Err(ScoreImportCredentialError::MissingApiKey {
                endpoint: ScoreImportEndpoint::GrooveStats,
            })
        );
        assert_eq!(
            validate_score_import_credentials(ScoreImportEndpoint::BoogieStats, "key", ""),
            Err(ScoreImportCredentialError::MissingUsername {
                endpoint: ScoreImportEndpoint::BoogieStats,
            })
        );
        assert!(
            validate_score_import_credentials(ScoreImportEndpoint::ArrowCloud, "key", "").is_ok()
        );
        assert!(
            validate_score_import_credentials(ScoreImportEndpoint::GrooveStats, "key", "player")
                .is_ok()
        );
    }

    #[test]
    fn score_import_credential_errors_use_context_messages() {
        let request_error = ScoreImportCredentialError::MissingApiKey {
            endpoint: ScoreImportEndpoint::GrooveStats,
        };
        assert_eq!(
            request_error.request_message(),
            "GrooveStats API key is missing in profile configuration."
        );
        assert_eq!(
            request_error.import_message(),
            "GrooveStats API key is not set in profile configuration."
        );

        let import_error = ScoreImportCredentialError::MissingUsername {
            endpoint: ScoreImportEndpoint::BoogieStats,
        };
        assert_eq!(
            import_error.request_message(),
            "BoogieStats username is missing in profile configuration."
        );
        assert_eq!(
            import_error.import_message(),
            "BoogieStats username is not set in profile configuration."
        );
    }

    #[test]
    fn imported_player_score_tracks_chart_stats_need() {
        let mut score = ImportedPlayerScore {
            score_10000: 9876.0,
            comments: Some("[DS], 2e".to_string()),
            is_fail: false,
            ex_evidence: GsExEvidence::default(),
        };
        assert!(score.needs_chart_stats());

        score.score_10000 = 10_000.0;
        assert!(!score.needs_chart_stats());

        score.score_10000 = 9876.0;
        score.comments = None;
        assert!(!score.needs_chart_stats());
    }

    #[test]
    fn cached_score_import_result_includes_itl_self_data() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 9876.0,
                    comments: Some("[DS], 2e".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(None, Some("[DS], 2e")),
                }),
                score_proves_nonquint_ex: false,
                itl_self_score: Some(9912),
                itl_self_rank: Some(42),
                itl_self_found: true,
            },
            None,
        );

        assert!(result.score.is_some());
        assert!(result.itl_self_found);
        assert_eq!(result.itl_self_score, Some(9912));
        assert_eq!(result.itl_self_rank, Some(42));
    }

    #[test]
    fn cached_score_import_result_uses_ex_leaderboard_for_quint() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 10_000.0,
                    comments: Some("[DS], FA+, 99.78EX".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(
                        Some(10_000.0),
                        Some("[DS], FA+, 99.78EX"),
                    ),
                }),
                score_proves_nonquint_ex: false,
                itl_self_score: None,
                itl_self_rank: None,
                itl_self_found: false,
            },
            None,
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Quint);
        assert_eq!(score.lamp_index, Some(0));
    }

    #[test]
    fn cached_score_import_result_rejects_lower_ex_quint_comment() {
        let result = cached_score_import_result_from_imported(
            PlayerScoreImportResult {
                score: Some(ImportedPlayerScore {
                    score_10000: 10_000.0,
                    comments: Some("[DS], FA+, 100.00EX, C875".to_string()),
                    is_fail: false,
                    ex_evidence: GsExEvidence::from_sources(
                        Some(9_978.0),
                        Some("[DS], FA+, 100.00EX, C875"),
                    ),
                }),
                score_proves_nonquint_ex: true,
                itl_self_score: None,
                itl_self_rank: None,
                itl_self_found: false,
            },
            None,
        );

        let score = result.score.expect("score cached");
        assert_eq!(score.grade, Grade::Tier01);
        assert_eq!(score.lamp_index, Some(1));
        assert!(result.score_proves_nonquint_ex);
    }

    #[test]
    fn score_import_runner_tracks_progress_and_events() {
        let packs = vec![
            (
                "Pack A".to_string(),
                vec!["hit".to_string(), "missing".to_string()],
            ),
            ("Pack B".to_string(), vec!["fail".to_string()]),
        ];
        let mut stored = Vec::new();
        let mut progress = Vec::new();
        let mut events = Vec::new();

        let summary = run_score_import_pack_groups(
            ScoreImportEndpoint::GrooveStats,
            "player",
            packs,
            true,
            |chart_hash| match chart_hash {
                "hit" => Ok(CachedScoreImportResult {
                    score: Some(cached_score(Grade::Tier03, 0.9876, Some(1), None)),
                    score_proves_nonquint_ex: true,
                    itl_self_score: Some(9876),
                    itl_self_rank: Some(4),
                    itl_self_found: true,
                }),
                "missing" => Ok(CachedScoreImportResult {
                    score: None,
                    score_proves_nonquint_ex: false,
                    itl_self_score: None,
                    itl_self_rank: None,
                    itl_self_found: false,
                }),
                _ => Err("network".to_string()),
            },
            |chart_hash, result| {
                stored.push((chart_hash.to_string(), result.score.is_some()));
            },
            |p| progress.push(p),
            || false,
            |event| events.push(event),
        );

        assert_eq!(summary.requested_charts, 3);
        assert_eq!(summary.imported_scores, 1);
        assert_eq!(summary.missing_scores, 1);
        assert_eq!(summary.failed_requests, 1);
        assert!(!summary.canceled);
        assert_eq!(
            stored,
            vec![("hit".to_string(), true), ("missing".to_string(), false)]
        );
        assert_eq!(progress.len(), 4);
        assert_eq!(progress[0].processed_charts, 0);
        assert!(progress[0].detail.contains("missing only"));
        assert_eq!(progress[3].processed_charts, 3);
        assert_eq!(progress[3].failed_requests, 1);
        assert!(events.contains(&ScoreImportRunEvent::PackComplete {
            detail: "Pack 1/2 complete: Pack A (2 charts -> 1 hit, 1 missing).".to_string(),
        }));
        assert!(events.contains(&ScoreImportRunEvent::RequestFailed {
            import_name: "GrooveStats",
            chart_hash: "fail".to_string(),
            error: "network".to_string(),
        }));
    }

    #[test]
    fn validated_score_import_rejects_missing_credentials() {
        let result = run_validated_score_import(
            ValidatedScoreImportInput {
                endpoint: ScoreImportEndpoint::GrooveStats,
                api_key: "",
                username: "player",
                pack_chart_groups: vec![("Pack".to_string(), vec!["hash".to_string()])],
                only_missing_scores: false,
            },
            |_| panic!("fetch should not run"),
            |_, _, _| panic!("score store should not run"),
            |_, _, _| panic!("itl store should not run"),
            |_| panic!("progress should not run"),
            || false,
            |_| panic!("event should not run"),
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "GrooveStats API key is not set in profile configuration."
        );
    }

    #[test]
    fn validated_score_import_dispatches_score_and_itl_results() {
        let mut stored_scores = Vec::new();
        let mut stored_itl = Vec::new();

        let summary = run_validated_score_import(
            ValidatedScoreImportInput {
                endpoint: ScoreImportEndpoint::GrooveStats,
                api_key: "key",
                username: "player",
                pack_chart_groups: vec![("Pack".to_string(), vec!["hit".to_string()])],
                only_missing_scores: false,
            },
            |_| {
                Ok(CachedScoreImportResult {
                    score: Some(cached_score(Grade::Tier03, 0.9876, Some(1), None)),
                    score_proves_nonquint_ex: true,
                    itl_self_score: Some(9876),
                    itl_self_rank: Some(4),
                    itl_self_found: true,
                })
            },
            |chart_hash, score, proves_nonquint| {
                stored_scores.push((chart_hash.to_string(), score.grade, proves_nonquint));
            },
            |chart_hash, score, rank| {
                stored_itl.push((chart_hash.to_string(), score, rank));
            },
            |_| {},
            || false,
            |_| {},
        )
        .expect("valid import should run");

        assert_eq!(summary.requested_charts, 1);
        assert_eq!(summary.imported_scores, 1);
        assert_eq!(
            stored_scores,
            vec![("hit".to_string(), Grade::Tier03, true)]
        );
        assert_eq!(stored_itl, vec![("hit".to_string(), Some(9876), Some(4))]);
    }

    #[test]
    fn arrowcloud_bulk_runner_tracks_chunk_progress_and_events() {
        let packs = vec![(
            "Pack A".to_string(),
            vec!["hit".to_string(), "missing".to_string(), "fail".to_string()],
        )];
        let mut progress = Vec::new();
        let mut events = Vec::new();

        let summary = run_arrowcloud_bulk_import_pack_groups(
            packs,
            2,
            false,
            |chunk| {
                if chunk.iter().any(|hash| hash == "fail") {
                    return Err("network".to_string());
                }
                Ok(ArrowCloudBulkChunkResult { hits: 1, misses: 1 })
            },
            |p| progress.push(p),
            || false,
            |event| events.push(event),
        );

        assert_eq!(summary.requested_charts, 3);
        assert_eq!(summary.imported_scores, 1);
        assert_eq!(summary.missing_scores, 1);
        assert_eq!(summary.failed_requests, 1);
        assert!(!summary.canceled);
        assert_eq!(progress.len(), 3);
        assert_eq!(progress[0].processed_charts, 0);
        assert_eq!(progress[1].processed_charts, 2);
        assert_eq!(progress[2].processed_charts, 3);
        assert_eq!(progress[2].failed_requests, 1);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ArrowCloudBulkImportRunEvent::ChunkSucceeded {
                    pack_name,
                    chunk_len: 2,
                    hits: 1,
                    misses: 1,
                    ..
                } if pack_name == "Pack A"
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ArrowCloudBulkImportRunEvent::RequestFailed { detail }
                    if detail.contains("network")
            )
        }));
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ArrowCloudBulkImportRunEvent::PackComplete { detail }
                    if detail.contains("1 failed")
            )
        }));
    }

    #[test]
    fn itl_eval_state_defaults_to_inactive() {
        let state = ItlEvalState::default();
        assert!(!state.active);
        assert!(!state.eligible);
        assert!(!state.chart_no_cmod);
        assert!(!state.used_cmod);
        assert!(state.reason_lines.is_empty());
    }

    #[test]
    fn groovestats_eval_state_defaults_to_invalid() {
        let state = GrooveStatsEvalState::default();
        assert!(!state.valid);
        assert!(state.reason_lines.is_empty());
        assert!(state.manual_qr_url.is_none());
    }

    #[test]
    fn scorebox_profile_snapshot_defaults_to_inactive() {
        let snapshot = GameplayScoreboxProfileSnapshot::default();
        assert!(!snapshot.display_scorebox);
        assert!(!snapshot.gs_active);
        assert!(!snapshot.show_ex_score);
        assert!(snapshot.api_key().is_empty());
        assert!(snapshot.arrowcloud_api_key().is_empty());
        assert!(!snapshot.include_arrowcloud());
        assert!(snapshot.gs_username().is_empty());
        assert!(snapshot.persistent_profile_id().is_none());
        assert!(snapshot.auto_profile_id().is_none());
        assert!(!snapshot.should_auto_populate());
    }

    #[test]
    fn scorebox_snapshot_trims_and_gates_services() {
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            true,
            "  gs-key  ",
            "  ac-key  ",
            "  player  ",
            Some("profile-1".to_string()),
        );

        assert!(snapshot.display_scorebox);
        assert!(snapshot.gs_active);
        assert!(snapshot.show_ex_score);
        assert_eq!(snapshot.api_key(), "gs-key");
        assert_eq!(snapshot.arrowcloud_api_key(), "ac-key");
        assert!(snapshot.include_arrowcloud());
        assert_eq!(snapshot.gs_username(), "player");
        assert_eq!(snapshot.persistent_profile_id(), Some("profile-1"));
        assert_eq!(snapshot.auto_profile_id(), Some("profile-1"));
        assert!(snapshot.should_auto_populate());
    }

    #[test]
    fn scorebox_snapshot_requires_join_and_keys() {
        let not_joined = scorebox_snapshot(
            true,
            false,
            false,
            true,
            true,
            true,
            "gs-key",
            "ac-key",
            "player",
            Some("profile-1".to_string()),
        );
        assert!(!not_joined.gs_active);
        assert!(not_joined.include_arrowcloud());

        let missing_keys = scorebox_snapshot(
            true,
            false,
            true,
            true,
            true,
            true,
            "  ",
            "  ",
            "  ",
            Some("profile-1".to_string()),
        );
        assert!(!missing_keys.gs_active);
        assert!(!missing_keys.include_arrowcloud());
        assert_eq!(missing_keys.auto_profile_id(), Some("profile-1"));
        assert!(!missing_keys.should_auto_populate());
    }

    #[test]
    fn player_leaderboard_cache_key_uses_active_profile_snapshot() {
        let snapshot = scorebox_snapshot(
            true,
            true,
            true,
            true,
            true,
            false,
            "  gs-key  ",
            "  ac-key  ",
            "player",
            None,
        );
        let key =
            player_leaderboard_cache_key("  deadbeef  ", &snapshot).expect("active key expected");

        assert_eq!(key.chart_hash, "deadbeef");
        assert_eq!(key.api_key, "gs-key");
        assert_eq!(key.arrowcloud_api_key, "ac-key");
        assert!(key.include_arrowcloud);
        assert!(key.show_ex_score);
    }

    #[test]
    fn player_leaderboard_cache_key_requires_chart_and_gs() {
        let inactive = scorebox_snapshot(
            true, false, true, true, true, false, " ", "ac-key", "player", None,
        );
        assert!(player_leaderboard_cache_key("deadbeef", &inactive).is_none());

        let active = scorebox_snapshot(
            true, false, true, true, true, false, "gs-key", "ac-key", "player", None,
        );
        assert!(player_leaderboard_cache_key(" ", &active).is_none());
    }

    #[test]
    fn cached_leaderboard_snapshots_set_state_exclusively() {
        let data = PlayerLeaderboardData {
            panes: Vec::new(),
            srpg_self_score: Some(9_432),
            itl_self_score: Some(9_876),
            itl_self_rank: Some(2),
        };
        let loading = CachedPlayerLeaderboardData::loading();
        assert!(loading.loading);
        assert!(loading.data.is_none());
        assert!(loading.error.is_none());

        let ready = CachedPlayerLeaderboardData::ready(data);
        assert!(!ready.loading);
        assert!(ready.data.is_some());
        assert!(ready.error.is_none());

        let error = CachedPlayerLeaderboardData::error("offline".to_string());
        assert!(!error.loading);
        assert!(error.data.is_none());
        assert_eq!(error.error.as_deref(), Some("offline"));
    }
}
