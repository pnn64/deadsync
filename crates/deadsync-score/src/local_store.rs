use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::{
    ArrowCloudScores, CachedScore, Grade, GsScoreEntry, LeaderboardEntry, LocalScoreEntry,
    LocalScoreHeader, LocalScoreIndex, MachineBest, MachineLeaderboardPlay, MachineReplayEntry,
    MachineReplayPlay, cached_score_from_gs_entry, decode_gs_score_entry, decode_local_score_entry,
    decode_local_score_header, decode_local_score_index, encode_gs_score_entry,
    encode_local_score_entry, encode_local_score_index, fix_gs_cached_score, grade_from_code,
    gs_score_entry_from_cached, is_better_itg, machine_leaderboard_entries, machine_replay_entries,
    parse_score_file_name, score_file_shard, update_local_score_index,
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
        let Some((chart_hash, played_at_ms)) = parse_score_file_name(name) else {
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

pub fn collect_recent_local_plays_in_root(root: &Path, latest_by_chart: &mut HashMap<String, i64>) {
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

pub fn recent_played_chart_hashes_in_root(root: &Path) -> Vec<String> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut latest_by_chart: HashMap<String, i64> = HashMap::new();
    collect_recent_local_plays_in_root(root, &mut latest_by_chart);

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

pub fn recent_played_chart_hashes_in_profiles_root(profiles_root: &Path) -> Vec<String> {
    let Ok(read_dir) = fs::read_dir(profiles_root) else {
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
            collect_recent_local_plays_in_root(&local_root, &mut latest_by_chart);
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
        let Some((chart_hash, _played_at_ms)) = parse_score_file_name(name) else {
            continue;
        };
        counts_by_chart
            .entry(chart_hash.to_string())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
}

pub fn collect_local_play_counts_in_root(root: &Path, counts_by_chart: &mut HashMap<String, u32>) {
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

pub fn played_chart_counts_in_root(root: &Path) -> Vec<(String, u32)> {
    if !root.is_dir() {
        return Vec::new();
    }

    let mut counts_by_chart: HashMap<String, u32> = HashMap::new();
    collect_local_play_counts_in_root(root, &mut counts_by_chart);

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

pub fn played_chart_counts_in_profiles_root(profiles_root: &Path) -> Vec<(String, u32)> {
    let Ok(read_dir) = fs::read_dir(profiles_root) else {
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
            collect_local_play_counts_in_root(&local_root, &mut counts_by_chart);
        }
    }

    let mut ranked: Vec<(String, u32)> = counts_by_chart.into_iter().collect();
    ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
}

pub fn read_local_score_header(path: &Path) -> Option<LocalScoreHeader> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(1024);
    if file.take(1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
        return None;
    }
    decode_local_score_header(&buf)
}

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

pub fn machine_best_itg_from_profiles(
    profiles: &[LocalScoreProfileSource],
) -> HashMap<String, MachineBest> {
    let mut best_itg: HashMap<String, MachineBest> = HashMap::new();
    for profile in profiles {
        let idx = load_local_score_index_from_root(&profile.root);
        for (chart_hash, score) in idx.best_itg {
            match best_itg.get_mut(&chart_hash) {
                Some(existing) => {
                    if is_better_itg(&score, &existing.score) {
                        existing.score = score;
                        existing.initials.clone_from(&profile.initials);
                    }
                }
                None => {
                    best_itg.insert(
                        chart_hash,
                        MachineBest {
                            score,
                            initials: profile.initials.clone(),
                        },
                    );
                }
            }
        }
    }
    best_itg
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
