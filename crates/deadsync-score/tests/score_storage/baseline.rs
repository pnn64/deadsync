// Frozen from 67a81510d / 0.5.1220. Keep original algorithms and attributes.
use super::*;

#[allow(clippy::too_many_arguments)]
fn push_local_leaderboard_candidates_from_dir<'a>(
    dir: &Path,
    chart_hash: &str,
    name: &'a str,
    machine_tag: Option<&'a str>,
    max_entries: usize,
    header_buf: &mut Vec<u8>,
    next_ordinal: &mut usize,
    out: &mut Vec<LocalLeaderboardCandidate<'a>>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_score_file_name(file_name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(header) = read_local_score_header_into(&path, header_buf) else {
            continue;
        };
        out.push(LocalLeaderboardCandidate {
            name,
            machine_tag,
            score_percent: header.score_percent,
            played_at_ms,
            is_fail: grade_from_code(header.grade_code) == Grade::Failed
                || header.fail_time.is_some(),
            ordinal: *next_ordinal,
        });
        *next_ordinal = next_ordinal.saturating_add(1);

        if out.len() > max_entries.saturating_mul(2) {
            truncate_to_best(out, max_entries, compare_local_leaderboard_candidates);
        }
    }
}

fn push_local_replay_candidates_from_dir<'a>(
    dir: &Path,
    chart_hash: &str,
    initials: &'a str,
    header_buf: &mut Vec<u8>,
    next_ordinal: &mut usize,
    out: &mut Vec<LocalReplayCandidate<'a>>,
) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((file_hash, played_at_ms)) = parse_score_file_name(file_name) else {
            continue;
        };
        if file_hash != chart_hash {
            continue;
        }
        let Some(header) = read_local_score_header_into(&path, header_buf) else {
            continue;
        };
        out.push(LocalReplayCandidate {
            initials,
            path,
            score_percent: header.score_percent,
            played_at_ms,
            ordinal: *next_ordinal,
        });
        *next_ordinal = next_ordinal.saturating_add(1);
    }
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

#[must_use]
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

// Adapters below this line are test-only, outside the frozen source blocks.
pub(super) fn leaderboard(
    dir: &Path,
    hash: &str,
    max: usize,
) -> Vec<LocalLeaderboardCandidate<'static>> {
    let mut out = Vec::new();
    push_local_leaderboard_candidates_from_dir(
        dir,
        hash,
        "Player",
        Some("Tag"),
        max,
        &mut Vec::with_capacity(1024),
        &mut 7,
        &mut out,
    );
    out
}
pub(super) fn replay(dir: &Path, hash: &str) -> Vec<LocalReplayCandidate<'static>> {
    let mut out = Vec::new();
    push_local_replay_candidates_from_dir(
        dir,
        hash,
        "AAA",
        &mut Vec::with_capacity(1024),
        &mut 7,
        &mut out,
    );
    out
}
pub(super) fn cached(bytes: &[u8]) -> Option<CachedScore> {
    decode_gs_score_entry(bytes)
        .as_ref()
        .map(cached_score_from_gs_entry)
}
pub(super) fn duplicate(dir: &Path, hash: &str, new_entry: &GsScoreEntry) -> bool {
    let entries = gs_entries_for_chart(hash, dir);
    let epsilon = 1e-9_f64;
    for existing in &entries {
        if existing.username.eq_ignore_ascii_case(&new_entry.username)
            && (existing.score_percent - new_entry.score_percent).abs() <= epsilon
            && existing.lamp_index == new_entry.lamp_index
            && existing.lamp_judge_count == new_entry.lamp_judge_count
            && existing.grade_code == new_entry.grade_code
        {
            return true;
        }
    }
    false
}

// Corruption tests bound the old owning decoder: malformed lengths can otherwise
// request terabytes before it discovers the slice is too short. Timings use
// the original, unbounded decoder above. All valid test payloads fit this limit.
pub(super) fn decode_bounded(bytes: &[u8]) -> Option<GsScoreEntry> {
    if let Ok((entry, _)) = bincode::decode_from_slice::<GsScoreEntry, _>(
        bytes,
        bincode::config::standard().with_limit::<16384>(),
    ) {
        return Some(entry);
    }
    if let Ok((v1, _)) = bincode::decode_from_slice::<GsScoreEntryV1, _>(
        bytes,
        bincode::config::standard().with_limit::<16384>(),
    ) {
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
