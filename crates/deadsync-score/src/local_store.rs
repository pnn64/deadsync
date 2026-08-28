use rustc_hash::FxBuildHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::BuildHasher;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{
    ArrowCloudScores, CachedScore, Grade, GsScoreEntry, LeaderboardEntry, LocalScoreEntry,
    LocalScoreHeader, LocalScoreIndex, MachineBest, MachineBestScalar, MachineLeaderboardPlay,
    MachineLocalScoreBests, MachineReplayEntry, MachineReplayPlay, cached_score_from_gs_entry,
    decode_gs_score_entry, decode_local_score_entry, decode_local_score_header,
    decode_local_score_index, encode_gs_score_entry, encode_local_score_entry,
    encode_local_score_index, fix_gs_cached_score, grade_from_code, gs_score_entry_from_cached,
    is_better_itg, machine_leaderboard_entries, machine_replay_entries, parse_score_file_name,
    score_file_shard, update_local_score_index,
};

#[derive(Debug)]
pub enum ScoreStoreWriteStatus {
    SkippedDuplicate,
    Written(PathBuf),
}

#[derive(Debug)]
pub enum ScoreStoreWriteError {
    CreateDir {
        dir: PathBuf,
        error: std::io::Error,
    },
    Encode {
        chart_hash: String,
    },
    WriteFile {
        path: PathBuf,
        error: std::io::Error,
    },
    CommitFile {
        path: PathBuf,
        tmp_path: PathBuf,
        error: std::io::Error,
    },
}

#[derive(Debug)]
pub enum ScoreIndexWriteError {
    CreateDir {
        dir: PathBuf,
        error: std::io::Error,
    },
    Encode {
        path: PathBuf,
    },
    WriteTemp {
        tmp_path: PathBuf,
        error: std::io::Error,
    },
    Commit {
        path: PathBuf,
        tmp_path: PathBuf,
        error: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreProfilePaths {
    profile_dir: PathBuf,
}

#[derive(Debug)]
pub struct GsScoreCacheLoad {
    pub by_chart: HashMap<String, CachedScore>,
    pub write_error: Option<ScoreIndexWriteError>,
}

#[derive(Debug)]
pub struct LocalScoreCacheLoad {
    pub index: LocalScoreIndex,
    pub best_itg_count: usize,
    pub best_ex_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LocalScoreAppendRecord {
    pub path: PathBuf,
    pub header: LocalScoreHeader,
    pub cached_score: CachedScore,
}

impl ScoreProfilePaths {
    #[inline(always)]
    pub fn new(profile_dir: impl Into<PathBuf>) -> Self {
        Self {
            profile_dir: profile_dir.into(),
        }
    }

    #[inline(always)]
    #[must_use]
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    #[inline(always)]
    #[must_use]
    pub fn scores_dir(&self) -> PathBuf {
        self.profile_dir.join("scores")
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_dir(&self) -> PathBuf {
        self.scores_dir().join("gs")
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_chart_dir(&self, chart_hash: &str) -> PathBuf {
        self.gs_dir().join(score_file_shard(chart_hash))
    }

    #[inline(always)]
    #[must_use]
    pub fn gs_index_path(&self) -> PathBuf {
        self.gs_dir().join("index.bin")
    }

    #[inline(always)]
    #[must_use]
    pub fn ac_dir(&self) -> PathBuf {
        self.scores_dir().join("ac")
    }

    #[inline(always)]
    #[must_use]
    pub fn ac_index_path(&self) -> PathBuf {
        self.ac_dir().join("index.bin")
    }

    #[inline(always)]
    #[must_use]
    pub fn local_dir(&self) -> PathBuf {
        self.scores_dir().join("local")
    }

    #[inline(always)]
    #[must_use]
    pub fn local_index_path(&self) -> PathBuf {
        self.local_dir().join("index.bin")
    }
}

fn write_index_file<T: bincode::Encode>(
    path: &Path,
    value: &T,
) -> Result<(), ScoreIndexWriteError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| ScoreIndexWriteError::CreateDir {
        dir: parent.to_path_buf(),
        error,
    })?;
    let buf = bincode::encode_to_vec(value, bincode::config::standard()).map_err(|_| {
        ScoreIndexWriteError::Encode {
            path: path.to_path_buf(),
        }
    })?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, buf).map_err(|error| ScoreIndexWriteError::WriteTemp {
        tmp_path: tmp_path.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ScoreIndexWriteError::Commit {
            path: path.to_path_buf(),
            tmp_path,
            error,
        });
    }
    Ok(())
}

#[must_use]
pub fn load_gs_score_index_file(path: &Path) -> Option<(HashMap<String, CachedScore>, bool)> {
    let bytes = fs::read(path).ok()?;
    let (mut by_chart, _) = bincode::decode_from_slice::<HashMap<String, CachedScore>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    let mut changed = false;
    for score in by_chart.values_mut() {
        let fixed = fix_gs_cached_score(*score);
        changed |= fixed != *score;
        *score = fixed;
    }
    Some((by_chart, changed))
}

pub fn save_gs_score_index_file(
    path: &Path,
    by_chart: &HashMap<String, CachedScore>,
) -> Result<(), ScoreIndexWriteError> {
    write_index_file(path, by_chart)
}

#[must_use]
pub fn load_gs_score_index_or_scan(
    paths: &ScoreProfilePaths,
) -> (HashMap<String, CachedScore>, Option<ScoreIndexWriteError>) {
    let index_path = paths.gs_index_path();
    if let Some((by_chart, changed)) = load_gs_score_index_file(&index_path) {
        let write_error = changed
            .then(|| save_gs_score_index_file(&index_path, &by_chart).err())
            .flatten();
        return (by_chart, write_error);
    }

    let scanned = best_gs_scores_from_dir(&paths.gs_dir());
    let write_error = save_gs_score_index_file(&index_path, &scanned).err();
    (scanned, write_error)
}

#[must_use]
pub fn load_gs_score_cache_from_paths(paths: &ScoreProfilePaths) -> GsScoreCacheLoad {
    let (by_chart, write_error) = load_gs_score_index_or_scan(paths);
    GsScoreCacheLoad {
        by_chart,
        write_error,
    }
}

#[must_use]
pub fn load_ac_score_index_file(path: &Path) -> Option<HashMap<String, ArrowCloudScores>> {
    let bytes = fs::read(path).ok()?;
    let (by_chart, _) = bincode::decode_from_slice::<HashMap<String, ArrowCloudScores>, _>(
        &bytes,
        bincode::config::standard(),
    )
    .ok()?;
    Some(by_chart)
}

pub fn save_ac_score_index_file(
    path: &Path,
    by_chart: &HashMap<String, ArrowCloudScores>,
) -> Result<(), ScoreIndexWriteError> {
    write_index_file(path, by_chart)
}

#[must_use]
pub fn load_ac_score_index_for_profile(
    paths: &ScoreProfilePaths,
) -> HashMap<String, ArrowCloudScores> {
    load_ac_score_index_file(&paths.ac_index_path()).unwrap_or_default()
}

#[must_use]
pub fn load_local_score_index_file(path: &Path) -> Option<LocalScoreIndex> {
    let bytes = fs::read(path).ok()?;
    decode_local_score_index(&bytes)
}

pub fn save_local_score_index_file(
    path: &Path,
    index: &LocalScoreIndex,
) -> Result<(), ScoreIndexWriteError> {
    let Some(buf) = encode_local_score_index(index) else {
        return Err(ScoreIndexWriteError::Encode {
            path: path.to_path_buf(),
        });
    };
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|error| ScoreIndexWriteError::CreateDir {
        dir: parent.to_path_buf(),
        error,
    })?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, buf).map_err(|error| ScoreIndexWriteError::WriteTemp {
        tmp_path: tmp_path.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ScoreIndexWriteError::Commit {
            path: path.to_path_buf(),
            tmp_path,
            error,
        });
    }
    Ok(())
}

#[must_use]
pub fn load_local_score_index_file_or_default(path: &Path) -> LocalScoreIndex {
    load_local_score_index_file(path).unwrap_or_default()
}

#[must_use]
pub fn load_local_score_cache_from_paths(paths: &ScoreProfilePaths) -> LocalScoreCacheLoad {
    let index = load_local_score_index_from_root(&paths.local_dir());
    LocalScoreCacheLoad {
        best_itg_count: index.best_itg.len(),
        best_ex_count: index.best_ex.len(),
        index,
    }
}

fn count_score_bins_in_dir(dir: &Path) -> u32 {
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

#[must_use]
pub fn total_local_score_bins_in_root(root: &Path) -> u32 {
    if !root.is_dir() {
        return 0;
    }

    let mut total = count_score_bins_in_dir(root);
    let Ok(read_dir) = fs::read_dir(root) else {
        return total;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total = total.saturating_add(count_score_bins_in_dir(&path));
        }
    }
    total
}

type FxMap<K, V> = HashMap<K, V, FxBuildHasher>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayedChartHistory {
    pub recent_chart_hashes: Vec<String>,
    pub played_chart_counts: Vec<(String, u32)>,
}

#[derive(Clone, Copy)]
struct ChartPlayHistory {
    latest_ms: i64,
    count: u32,
}

fn note_recent<S: BuildHasher>(
    latest_by_chart: &mut HashMap<String, i64, S>,
    chart_hash: &str,
    played_at_ms: i64,
) {
    match latest_by_chart.get_mut(chart_hash) {
        Some(existing) => *existing = (*existing).max(played_at_ms),
        None => {
            latest_by_chart.insert(chart_hash.to_owned(), played_at_ms);
        }
    }
}

fn note_count<S: BuildHasher>(counts_by_chart: &mut HashMap<String, u32, S>, chart_hash: &str) {
    match counts_by_chart.get_mut(chart_hash) {
        Some(count) => *count = count.saturating_add(1),
        None => {
            counts_by_chart.insert(chart_hash.to_owned(), 1);
        }
    }
}

fn note_history(history_by_chart: &mut FxMap<String, ChartPlayHistory>, name: &str) {
    let Some((chart_hash, played_at_ms)) = parse_score_file_name(name) else {
        return;
    };
    match history_by_chart.get_mut(chart_hash) {
        Some(history) => {
            history.latest_ms = history.latest_ms.max(played_at_ms);
            history.count = history.count.saturating_add(1);
        }
        None => {
            history_by_chart.insert(
                chart_hash.to_owned(),
                ChartPlayHistory {
                    latest_ms: played_at_ms,
                    count: 1,
                },
            );
        }
    }
}

fn collect_recent_plays_in_dir<S: BuildHasher>(
    dir: &Path,
    latest_by_chart: &mut HashMap<String, i64, S>,
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
        let Some((chart_hash, played_at_ms)) = parse_score_file_name(name) else {
            continue;
        };
        note_recent(latest_by_chart, chart_hash, played_at_ms);
    }
}

pub fn collect_recent_local_plays_in_root<S: BuildHasher>(
    root: &Path,
    latest_by_chart: &mut HashMap<String, i64, S>,
) {
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

fn collect_history_in_dir(dir: &Path, history_by_chart: &mut FxMap<String, ChartPlayHistory>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        note_history(history_by_chart, name);
    }
}

fn collect_local_history_in_root(
    root: &Path,
    history_by_chart: &mut FxMap<String, ChartPlayHistory>,
) {
    collect_history_in_dir(root, history_by_chart);
    let Ok(read_dir) = fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_history_in_dir(&path, history_by_chart);
        }
    }
}

fn rank_recent<S: BuildHasher>(latest_by_chart: HashMap<String, i64, S>) -> Vec<String> {
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

fn rank_counts<S: BuildHasher>(counts_by_chart: HashMap<String, u32, S>) -> Vec<(String, u32)> {
    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

fn rank_history(history_by_chart: FxMap<String, ChartPlayHistory>) -> PlayedChartHistory {
    let mut recent: Vec<(String, ChartPlayHistory)> = history_by_chart.into_iter().collect();
    let mut played_chart_counts = Vec::with_capacity(recent.len());
    for (chart_hash, history) in &recent {
        played_chart_counts.push((chart_hash.clone(), history.count));
    }
    played_chart_counts.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    recent.sort_unstable_by(|a, b| {
        b.1.latest_ms
            .cmp(&a.1.latest_ms)
            .then_with(|| a.0.cmp(&b.0))
    });
    PlayedChartHistory {
        recent_chart_hashes: recent
            .into_iter()
            .map(|(chart_hash, _)| chart_hash)
            .collect(),
        played_chart_counts,
    }
}

#[must_use]
pub fn recent_played_chart_hashes_in_root(root: &Path) -> Vec<String> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut latest_by_chart = FxMap::default();
    collect_recent_local_plays_in_root(root, &mut latest_by_chart);
    rank_recent(latest_by_chart)
}

#[must_use]
pub fn recent_played_chart_hashes_in_profiles_root(profiles_root: &Path) -> Vec<String> {
    let Ok(read_dir) = fs::read_dir(profiles_root) else {
        return Vec::new();
    };

    let mut latest_by_chart = FxMap::default();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_recent_local_plays_in_root(&local_root, &mut latest_by_chart);
        }
    }

    rank_recent(latest_by_chart)
}

fn collect_play_counts_in_dir<S: BuildHasher>(
    dir: &Path,
    counts_by_chart: &mut HashMap<String, u32, S>,
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
        let Some((chart_hash, _played_at_ms)) = parse_score_file_name(name) else {
            continue;
        };
        note_count(counts_by_chart, chart_hash);
    }
}

pub fn collect_local_play_counts_in_root<S: BuildHasher>(
    root: &Path,
    counts_by_chart: &mut HashMap<String, u32, S>,
) {
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

#[must_use]
pub fn played_chart_counts_in_root(root: &Path) -> Vec<(String, u32)> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut counts_by_chart = FxMap::default();
    collect_local_play_counts_in_root(root, &mut counts_by_chart);
    rank_counts(counts_by_chart)
}

#[must_use]
pub fn played_chart_counts_in_profiles_root(profiles_root: &Path) -> Vec<(String, u32)> {
    let Ok(read_dir) = fs::read_dir(profiles_root) else {
        return Vec::new();
    };

    let mut counts_by_chart = FxMap::default();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_local_play_counts_in_root(&local_root, &mut counts_by_chart);
        }
    }

    rank_counts(counts_by_chart)
}

#[must_use]
pub fn played_chart_history_in_root(root: &Path) -> PlayedChartHistory {
    if !root.is_dir() {
        return PlayedChartHistory::default();
    }
    let mut history_by_chart = FxMap::default();
    collect_local_history_in_root(root, &mut history_by_chart);
    rank_history(history_by_chart)
}

#[must_use]
pub fn played_chart_history_in_profiles_root(profiles_root: &Path) -> PlayedChartHistory {
    let Ok(read_dir) = fs::read_dir(profiles_root) else {
        return PlayedChartHistory::default();
    };
    let mut history_by_chart = FxMap::default();
    for entry in read_dir.flatten() {
        let profile_dir = entry.path();
        if !profile_dir.is_dir() {
            continue;
        }
        let local_root = profile_dir.join("scores").join("local");
        if local_root.is_dir() {
            collect_local_history_in_root(&local_root, &mut history_by_chart);
        }
    }
    rank_history(history_by_chart)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn benchmark_play_counts_from_names(names: &[String]) -> Vec<(String, u32)> {
    let mut counts_by_chart = FxMap::default();
    for name in names {
        if let Some((chart_hash, _)) = parse_score_file_name(name) {
            note_count(&mut counts_by_chart, chart_hash);
        }
    }
    rank_counts(counts_by_chart)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn benchmark_recent_from_names(names: &[String]) -> Vec<String> {
    let mut latest_by_chart = FxMap::default();
    for name in names {
        if let Some((chart_hash, played_at_ms)) = parse_score_file_name(name) {
            note_recent(&mut latest_by_chart, chart_hash, played_at_ms);
        }
    }
    rank_recent(latest_by_chart)
}

#[cfg(any(test, feature = "bench-support"))]
#[doc(hidden)]
#[must_use]
pub fn benchmark_history_from_names(names: &[String]) -> PlayedChartHistory {
    let mut history_by_chart = FxMap::default();
    for name in names {
        note_history(&mut history_by_chart, name);
    }
    rank_history(history_by_chart)
}

#[must_use]
pub fn read_local_score_header(path: &Path) -> Option<LocalScoreHeader> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(1024);
    if file.take(1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return None;
    }
    decode_local_score_header(&buf)
}

#[must_use]
pub fn read_local_score_entry(path: &Path) -> Option<LocalScoreEntry> {
    let bytes = fs::read(path).ok()?;
    decode_local_score_entry(&bytes)
}

pub fn scan_local_scores_dir(dir: &Path, index: &mut LocalScoreIndex) {
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
        let Some((chart_hash, _played_at_ms)) = parse_score_file_name(name) else {
            continue;
        };
        let Some(header) = read_local_score_header(&path) else {
            continue;
        };

        update_local_score_index(index, chart_hash, &header);
    }
}

#[must_use]
pub fn load_local_score_index_from_root(root: &Path) -> LocalScoreIndex {
    if !root.is_dir() {
        return LocalScoreIndex::default();
    }
    let index_path = root.join("index.bin");
    if let Some(index) = load_local_score_index_file(&index_path) {
        return index;
    }

    let mut index = LocalScoreIndex::default();

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

    let _ = save_local_score_index_file(&index_path, &index);
    index
}

pub fn push_local_leaderboard_plays_from_dir(
    dir: &Path,
    chart_hash: &str,
    name: &str,
    machine_tag: Option<&str>,
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
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_score_file_name(file_name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(header) = read_local_score_header(&path) else {
            continue;
        };
        out.push(MachineLeaderboardPlay {
            name: name.to_string(),
            machine_tag: machine_tag.map(str::to_string),
            score_percent: header.score_percent,
            played_at_ms,
            is_fail: grade_from_code(header.grade_code) == Grade::Failed
                || header.fail_time.is_some(),
        });
    }
}

pub fn push_local_replay_plays_from_dir(
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
        let Some((file_hash, played_at_ms)) = parse_score_file_name(name) else {
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
            replay_beat0_time_ns: full.beat0_time_ns,
            replay: full.replay,
        });
    }
}

#[derive(Clone, Debug)]
pub struct LocalScoreProfileSource {
    pub root: PathBuf,
    pub initials: String,
    pub display_name: String,
}

#[must_use]
pub fn machine_local_score_bests_from_profiles(
    profiles: &[LocalScoreProfileSource],
) -> MachineLocalScoreBests {
    let mut best = MachineLocalScoreBests::default();
    for profile in profiles {
        let idx = load_local_score_index_from_root(&profile.root);
        for (chart_hash, score) in idx.best_itg {
            match best.itg.get_mut(&chart_hash) {
                Some(existing) => {
                    if is_better_itg(&score, &existing.score) {
                        existing.score = score;
                        existing.initials.clone_from(&profile.initials);
                    }
                }
                None => {
                    best.itg.insert(
                        chart_hash,
                        MachineBest {
                            score,
                            initials: profile.initials.clone(),
                        },
                    );
                }
            }
        }
        merge_machine_scalars(&mut best.ex, idx.best_ex, profile.initials.as_str());
        merge_machine_scalars(
            &mut best.hard_ex,
            idx.best_hard_ex,
            profile.initials.as_str(),
        );
    }
    best
}

fn merge_machine_scalars(
    machine: &mut HashMap<String, MachineBestScalar>,
    profile: HashMap<String, crate::LocalScoreBestScalar>,
    initials: &str,
) {
    for (chart_hash, score) in profile {
        match machine.get_mut(&chart_hash) {
            Some(existing)
                if crate::is_better_scalar_score(
                    score.grade,
                    score.percent,
                    existing.score.grade,
                    existing.score.percent,
                ) =>
            {
                existing.score = score;
                existing.initials = initials.to_string();
            }
            Some(_) => {}
            None => {
                machine.insert(
                    chart_hash,
                    MachineBestScalar {
                        score,
                        initials: initials.to_string(),
                    },
                );
            }
        }
    }
}

fn push_local_leaderboard_plays_from_root(
    root: &Path,
    chart_hash: &str,
    name: &str,
    machine_tag: Option<&str>,
    out: &mut Vec<MachineLeaderboardPlay>,
) {
    push_local_leaderboard_plays_from_dir(root, chart_hash, name, machine_tag, out);
    push_local_leaderboard_plays_from_dir(
        &local_score_shard_dir(root, chart_hash),
        chart_hash,
        name,
        machine_tag,
        out,
    );
}

#[must_use]
pub fn machine_leaderboard_local_from_profiles(
    profiles: &[LocalScoreProfileSource],
    chart_hash: &str,
    max_entries: usize,
    use_display_names: bool,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays = Vec::new();
    for profile in profiles {
        if use_display_names {
            push_local_leaderboard_plays_from_root(
                &profile.root,
                chart_hash,
                profile.display_name.as_str(),
                Some(profile.initials.as_str()),
                &mut plays,
            );
        } else {
            push_local_leaderboard_plays_from_root(
                &profile.root,
                chart_hash,
                profile.initials.as_str(),
                None,
                &mut plays,
            );
        }
    }
    machine_leaderboard_entries(plays, max_entries)
}

#[must_use]
pub fn personal_leaderboard_local_from_root(
    root: &Path,
    chart_hash: &str,
    initials: &str,
    max_entries: usize,
) -> Vec<LeaderboardEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays = Vec::new();
    push_local_leaderboard_plays_from_root(root, chart_hash, initials, None, &mut plays);
    machine_leaderboard_entries(plays, max_entries)
}

#[must_use]
pub fn machine_replays_local_from_profiles(
    profiles: &[LocalScoreProfileSource],
    chart_hash: &str,
    max_entries: usize,
) -> Vec<MachineReplayEntry> {
    if chart_hash.trim().is_empty() || max_entries == 0 {
        return Vec::new();
    }

    let mut plays = Vec::new();
    for profile in profiles {
        push_local_replay_plays_from_dir(
            &profile.root,
            chart_hash,
            profile.initials.as_str(),
            &mut plays,
        );
        push_local_replay_plays_from_dir(
            &local_score_shard_dir(&profile.root, chart_hash),
            chart_hash,
            profile.initials.as_str(),
            &mut plays,
        );
    }
    machine_replay_entries(plays, max_entries)
}

#[must_use]
pub fn local_score_shard_dir(root: &Path, chart_hash: &str) -> PathBuf {
    root.join(score_file_shard(chart_hash))
}

pub fn write_local_score_entry_file(
    dir: &Path,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
) -> Result<PathBuf, ScoreStoreWriteError> {
    fs::create_dir_all(dir).map_err(|error| ScoreStoreWriteError::CreateDir {
        dir: dir.to_path_buf(),
        error,
    })?;

    let mut played_at_ms = entry.played_at_ms;
    let mut path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    while path.exists() {
        played_at_ms = played_at_ms.saturating_add(1);
        path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    }
    entry.played_at_ms = played_at_ms;

    let tmp_path = dir.join(format!(".{chart_hash}-{played_at_ms}.tmp"));
    let Some(buf) = encode_local_score_entry(entry) else {
        return Err(ScoreStoreWriteError::Encode {
            chart_hash: chart_hash.to_string(),
        });
    };
    fs::write(&tmp_path, buf).map_err(|error| ScoreStoreWriteError::WriteFile {
        path: tmp_path.clone(),
        error,
    })?;
    if let Err(error) = fs::rename(&tmp_path, &path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(ScoreStoreWriteError::CommitFile {
            path,
            tmp_path,
            error,
        });
    }

    Ok(path)
}

pub fn write_local_score_entry_for_profile(
    paths: &ScoreProfilePaths,
    chart_hash: &str,
    entry: &mut LocalScoreEntry,
) -> Result<LocalScoreAppendRecord, ScoreStoreWriteError> {
    let dir = local_score_shard_dir(&paths.local_dir(), chart_hash);
    let path = write_local_score_entry_file(&dir, chart_hash, entry)?;
    let header = entry.header();
    Ok(LocalScoreAppendRecord {
        path,
        header,
        cached_score: crate::cached_score_from_local_header(&header),
    })
}

pub fn save_local_score_index_after_append(
    paths: &ScoreProfilePaths,
    chart_hash: &str,
    header: &LocalScoreHeader,
    loaded_snapshot: Option<&LocalScoreIndex>,
) -> Result<(), ScoreIndexWriteError> {
    let index_path = paths.local_index_path();
    if let Some(index) = loaded_snapshot {
        return save_local_score_index_file(&index_path, index);
    }

    let mut index = load_local_score_index_file_or_default(&index_path);
    update_local_score_index(&mut index, chart_hash, header);
    save_local_score_index_file(&index_path, &index)
}

pub fn import_local_scores_with_writer<F, C, W>(
    scores: &mut [(String, LocalScoreEntry)],
    mut on_progress: F,
    should_cancel: C,
    mut write_score: W,
) -> (usize, bool)
where
    F: FnMut(usize, usize),
    C: Fn() -> bool,
    W: FnMut(&str, &mut LocalScoreEntry) -> bool,
{
    let total = scores.len();
    let mut written = 0usize;
    for (idx, (chart_hash, entry)) in scores.iter_mut().enumerate() {
        if should_cancel() {
            return (written, true);
        }
        if write_score(chart_hash, entry) {
            written += 1;
        }
        on_progress(idx + 1, total);
    }
    (written, false)
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
        let cached = cached_score_from_gs_entry(&entry);

        match best_by_chart.get_mut(chart_hash) {
            Some(existing) => {
                if is_better_itg(&cached, existing) {
                    *existing = cached;
                }
            }
            None => {
                best_by_chart.insert(chart_hash.to_string(), cached);
            }
        }
    }
}

#[must_use]
pub fn best_gs_scores_from_dir(dir: &Path) -> HashMap<String, CachedScore> {
    let mut best_by_chart: HashMap<String, CachedScore> = HashMap::new();

    if !dir.is_dir() {
        return best_by_chart;
    }

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

#[must_use]
pub fn gs_entries_for_chart(chart_hash: &str, dir: &Path) -> Vec<GsScoreEntry> {
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

pub fn write_gs_score_entry_file(
    dir: &Path,
    chart_hash: &str,
    score: CachedScore,
    username: &str,
    fetched_at_ms: i64,
) -> Result<ScoreStoreWriteStatus, ScoreStoreWriteError> {
    if username.trim().is_empty() {
        return Ok(ScoreStoreWriteStatus::SkippedDuplicate);
    }

    let entries = gs_entries_for_chart(chart_hash, dir);
    let new_entry = gs_score_entry_from_cached(score, username, fetched_at_ms);
    let epsilon = 1e-9_f64;
    for existing in &entries {
        if existing.username.eq_ignore_ascii_case(username)
            && (existing.score_percent - new_entry.score_percent).abs() <= epsilon
            && existing.lamp_index == new_entry.lamp_index
            && existing.lamp_judge_count == new_entry.lamp_judge_count
            && existing.grade_code == new_entry.grade_code
        {
            return Ok(ScoreStoreWriteStatus::SkippedDuplicate);
        }
    }

    fs::create_dir_all(dir).map_err(|error| ScoreStoreWriteError::CreateDir {
        dir: dir.to_path_buf(),
        error,
    })?;

    let path = dir.join(format!("{chart_hash}-{fetched_at_ms}.bin"));
    let Some(buf) = encode_gs_score_entry(&new_entry) else {
        return Err(ScoreStoreWriteError::Encode {
            chart_hash: chart_hash.to_string(),
        });
    };
    fs::write(&path, buf).map_err(|error| ScoreStoreWriteError::WriteFile {
        path: path.clone(),
        error,
    })?;

    Ok(ScoreStoreWriteStatus::Written(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempTree(PathBuf);

    impl TempTree {
        fn new(label: &str) -> Self {
            let id = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "deadsync-score-{label}-{}-{id}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test temp directory should be creatable");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn touch(dir: &Path, name: &str) {
        fs::create_dir_all(dir).expect("score shard should be creatable");
        fs::write(dir.join(name), []).expect("score fixture should be writable");
    }

    #[test]
    fn combined_history_matches_independent_rankings() {
        let names = [
            "alpha-100.bin",
            "alpha-300.bin",
            "beta-250.bin",
            "beta-200.bin",
            "gamma-300.bin",
            "invalid.bin",
            "ignored.txt",
        ]
        .map(str::to_owned);

        let history = benchmark_history_from_names(&names);
        assert_eq!(
            history.recent_chart_hashes,
            benchmark_recent_from_names(&names)
        );
        assert_eq!(
            history.played_chart_counts,
            benchmark_play_counts_from_names(&names)
        );
        assert_eq!(history.recent_chart_hashes, ["alpha", "gamma", "beta"]);
        assert_eq!(
            history.played_chart_counts,
            [
                ("alpha".to_owned(), 2),
                ("beta".to_owned(), 2),
                ("gamma".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn combined_history_preserves_root_and_shard_behavior() {
        let tree = TempTree::new("history-root");
        touch(tree.path(), "delta-150.bin");
        touch(&tree.path().join("aa"), "alpha-100.bin");
        touch(&tree.path().join("aa"), "alpha-300.bin");
        touch(&tree.path().join("bb"), "beta-250.bin");
        touch(&tree.path().join("bb"), "beta-200.bin");
        touch(&tree.path().join("cc"), "gamma-300.bin");
        touch(&tree.path().join("cc"), "ignored.txt");

        let history = played_chart_history_in_root(tree.path());
        assert_eq!(
            history.recent_chart_hashes,
            recent_played_chart_hashes_in_root(tree.path())
        );
        assert_eq!(
            history.played_chart_counts,
            played_chart_counts_in_root(tree.path())
        );
        assert_eq!(
            history.recent_chart_hashes,
            ["alpha", "gamma", "beta", "delta"]
        );
    }

    #[test]
    fn combined_machine_history_preserves_profile_aggregation() {
        let tree = TempTree::new("history-profiles");
        let p1 = tree.path().join("p1").join("scores").join("local");
        let p2 = tree.path().join("p2").join("scores").join("local");
        touch(&p1.join("aa"), "alpha-100.bin");
        touch(&p1.join("bb"), "beta-300.bin");
        touch(&p2.join("aa"), "alpha-400.bin");
        touch(&p2.join("cc"), "gamma-200.bin");

        let history = played_chart_history_in_profiles_root(tree.path());
        assert_eq!(
            history.recent_chart_hashes,
            recent_played_chart_hashes_in_profiles_root(tree.path())
        );
        assert_eq!(
            history.played_chart_counts,
            played_chart_counts_in_profiles_root(tree.path())
        );
        assert_eq!(history.recent_chart_hashes, ["alpha", "beta", "gamma"]);
        assert_eq!(
            history.played_chart_counts,
            [
                ("alpha".to_owned(), 2),
                ("beta".to_owned(), 1),
                ("gamma".to_owned(), 1)
            ]
        );
    }
}
