// Frozen from b0a4c0c0d / 0.5.1222; unchanged algorithms.
use super::*;

pub fn scan_local_scores_dir(dir: &Path, index: &mut LocalScoreIndex) {
    let mut buf = Vec::with_capacity(1024);
    scan_local_scores_dir_into(dir, index, &mut buf);
}

fn scan_local_scores_dir_into(dir: &Path, index: &mut LocalScoreIndex, buf: &mut Vec<u8>) {
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
        let Some(header) = read_local_score_header_into(&path, buf) else {
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

    let mut buf = Vec::with_capacity(1024);
    scan_local_scores_dir_into(root, &mut index, &mut buf);
    let Ok(read_dir) = fs::read_dir(root) else {
        return index;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_local_scores_dir_into(&path, &mut index, &mut buf);
        }
    }

    let _ = save_local_score_index_file(&index_path, &index);
    index
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
        let Some(entry) = crate::decode_gs_score_entry_ref(&bytes) else {
            continue;
        };
        let cached = entry.cached_score();

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

pub fn groovestats_reason_lines(checks: &[bool; GROOVESTATS_REASON_COUNT]) -> Vec<String> {
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
            6 => out.push("Music rate must be between 1.0x and 3.0x.".to_string()),
            7 => out.push("Note-removal modifiers are enabled.".to_string()),
            8 => out.push("Note-insertion modifiers are enabled.".to_string()),
            9 => out.push("Fail type must be Immediate or ImmediateContinue.".to_string()),
            10 => out.push("Autoplay or replay is not allowed.".to_string()),
            11 => out.push("MinTNSToScoreNotes cannot be W1 or W2.".to_string()),
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
    checks[6] = (1.0..=3.0).contains(&rate);
    checks[7] = (input.remove_mask & GS_INVALID_REMOVE_MASK) == 0;
    checks[8] = (input.insert_mask & GS_INVALID_INSERT_MASK) == 0;
    checks[9] = input.fail_type_ok;
    checks[10] = !input.autoplay_used;
    checks[11] = true;
    if (input.holds_mask & GS_INVALID_HOLDS_MASK) != 0 {
        checks[7] = false;
    }

    GrooveStatsEvalState {
        valid: checks.iter().all(|passed| *passed),
        reason_lines: groovestats_reason_lines(&checks),
        manual_qr_url: None,
    }
}

pub fn invalidate_chart_for_api(
    state: &mut PlayerLeaderboardCacheState,
    api_key: &str,
    chart_hash: &str,
    invalidated_at: Instant,
) {
    let matching_keys: HashSet<PlayerLeaderboardCacheKey> = state
        .by_key
        .keys()
        .chain(state.in_flight.keys())
        .chain(state.pending_refresh.keys())
        .chain(state.invalidated_after.keys())
        .filter(|key| key.api_key == api_key && key.chart_hash.eq_ignore_ascii_case(chart_hash))
        .cloned()
        .collect();
    for key in matching_keys {
        state.by_key.remove(&key);
        state.in_flight.remove(&key);
        state.pending_refresh.remove(&key);
        state.invalidated_after.insert(key, invalidated_at);
    }
}
