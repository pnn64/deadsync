// Frozen from c065be34c / 0.5.1221; original function bodies.
use super::*;

pub fn select_music_scorebox_filtered_panes(
    panes: &[LeaderboardPane],
    filter: SelectMusicScoreboxFilter,
) -> Vec<&LeaderboardPane> {
    let mut out = Vec::with_capacity(panes.len());
    for pane in panes {
        if select_music_scorebox_filter_allows_kind(scorebox_pane_kind(pane), filter) {
            out.push(pane);
        }
    }
    out
}

pub fn preferred_primary_scorebox_pane<'a>(
    panes: &'a [&'a LeaderboardPane],
    show_ex: bool,
) -> Option<&'a LeaderboardPane> {
    let want = if show_ex {
        ScoreboxPaneKind::Ex
    } else {
        ScoreboxPaneKind::Gs
    };
    panes
        .iter()
        .copied()
        .find(|pane| scorebox_pane_kind(pane) == want)
        .or_else(|| {
            panes
                .iter()
                .copied()
                .find(|pane| scorebox_pane_kind(pane) == ScoreboxPaneKind::Gs)
        })
        .or_else(|| {
            panes
                .iter()
                .copied()
                .find(|pane| scorebox_pane_kind(pane) == ScoreboxPaneKind::Ex)
        })
        .or_else(|| panes.first().copied())
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

pub fn unicode_case_insensitive_cmp(left: &str, right: &str) -> Ordering {
    left.chars()
        .flat_map(char::to_lowercase)
        .cmp(right.chars().flat_map(char::to_lowercase))
}
