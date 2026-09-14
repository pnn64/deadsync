// Frozen from bb0126368 / 0.5.1219. Keep original algorithms and attributes.
use deadsync_score::LeaderboardEntry;
use smallvec::SmallVec;
use std::collections::HashSet;

const LEADERBOARD_MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn format_leaderboard_date_with_empty(date: &str, empty_text: &str) -> String {
    let trimmed = date.trim();
    if trimmed.is_empty() {
        return empty_text.to_string();
    }

    let ymd = trimmed.split_once(' ').map_or(trimmed, |(value, _)| value);
    let ymd = ymd.split_once('T').map_or(ymd, |(value, _)| value);
    let mut parts = ymd.split('-');
    let (Some(year), Some(month), Some(day)) = (parts.next(), parts.next(), parts.next()) else {
        return trimmed.to_string();
    };

    let Some(month_idx) = month
        .parse::<usize>()
        .ok()
        .and_then(|m| m.checked_sub(1))
        .filter(|m| *m < LEADERBOARD_MONTH_ABBR.len())
    else {
        return trimmed.to_string();
    };
    let Some(day_num) = day.parse::<u32>().ok().filter(|d| *d > 0) else {
        return trimmed.to_string();
    };

    format!(
        "{} {}, {}",
        LEADERBOARD_MONTH_ABBR[month_idx], day_num, year
    )
}

#[must_use]
pub fn format_leaderboard_date(date: &str) -> String {
    format_leaderboard_date_with_empty(date, "")
}

#[must_use]
pub fn format_leaderboard_date_or_placeholder(date: &str) -> String {
    format_leaderboard_date_with_empty(date, "----------")
}

#[inline(always)]
fn same_leaderboard_entry(a: &LeaderboardEntry, b: &LeaderboardEntry) -> bool {
    a.rank == b.rank && a.name.eq_ignore_ascii_case(b.name.as_str())
}

#[inline(always)]
fn selected_contains(selected: &[&LeaderboardEntry], entry: &LeaderboardEntry) -> bool {
    selected
        .iter()
        .any(|chosen| same_leaderboard_entry(chosen, entry))
}

#[inline(always)]
fn leaderboard_neighbor_key(entry: &LeaderboardEntry, self_rank: Option<u32>) -> (u32, u32) {
    self_rank.map_or((0, entry.rank), |rank| {
        (entry.rank.abs_diff(rank), entry.rank)
    })
}

pub type PrioritizedLeaderboardEntryRefs<'a> = SmallVec<[&'a LeaderboardEntry; 10]>;

fn fill_prioritized_entries<'a>(
    entries: &'a [LeaderboardEntry],
    selected: &mut PrioritizedLeaderboardEntryRefs<'a>,
    max_rows: usize,
    self_rank: Option<u32>,
    include: impl Fn(&LeaderboardEntry) -> bool,
) {
    if selected.len() >= max_rows {
        return;
    }

    let phase_start = selected.len();
    let phase_capacity = max_rows - phase_start;
    for entry in entries {
        if !include(entry) || selected_contains(selected.as_slice(), entry) {
            continue;
        }
        let key = leaderboard_neighbor_key(entry, self_rank);
        let phase_entries = &selected[phase_start..];
        let insert_offset = phase_entries
            .partition_point(|chosen| leaderboard_neighbor_key(chosen, self_rank) <= key);
        if phase_entries.len() < phase_capacity {
            selected.insert(phase_start + insert_offset, entry);
        } else if insert_offset < phase_capacity {
            selected.pop();
            selected.insert(phase_start + insert_offset, entry);
        }
    }
}

fn prioritized_leaderboard_entry_refs_with_neighbors(
    entries: &[LeaderboardEntry],
    max_rows: usize,
    nearest_self: bool,
) -> PrioritizedLeaderboardEntryRefs<'_> {
    if max_rows == 0 {
        return SmallVec::new();
    }
    if entries.len() <= max_rows {
        return entries.iter().collect();
    }

    let mut top = None;
    let mut best_self = None;
    let mut next_self = None;
    for entry in entries {
        if top.is_none_or(|current: &LeaderboardEntry| entry.rank < current.rank) {
            top = Some(entry);
        }
        if !entry.is_self
            || best_self.is_some_and(|current| same_leaderboard_entry(current, entry))
            || next_self.is_some_and(|current| same_leaderboard_entry(current, entry))
        {
            continue;
        }
        if best_self.is_none_or(|current: &LeaderboardEntry| entry.rank < current.rank) {
            next_self = best_self;
            best_self = Some(entry);
        } else if next_self.is_none_or(|current: &LeaderboardEntry| entry.rank < current.rank) {
            next_self = Some(entry);
        }
    }

    let mut selected = SmallVec::with_capacity(max_rows);
    let top = top.expect("non-empty leaderboard");
    selected.push(top);
    let self_entry = if best_self.is_some_and(|entry| same_leaderboard_entry(top, entry)) {
        next_self
    } else {
        best_self
    };
    let self_rank = if nearest_self {
        self_entry.or(best_self).map(|entry| entry.rank)
    } else {
        None
    };
    if let Some(self_entry) = self_entry {
        selected.push(self_entry);
    }
    fill_prioritized_entries(entries, &mut selected, max_rows, None, |entry| {
        entry.is_rival
    });
    fill_prioritized_entries(entries, &mut selected, max_rows, self_rank, |_| true);
    selected.sort_unstable_by_key(|entry| entry.rank);
    selected
}

#[must_use]
pub fn prioritized_leaderboard_entry_refs(
    entries: &[LeaderboardEntry],
    max_rows: usize,
) -> PrioritizedLeaderboardEntryRefs<'_> {
    prioritized_leaderboard_entry_refs_with_neighbors(entries, max_rows, false)
}

/// Keeps the world record, self, and rivals visible, then fills the remaining
/// rows with the entries nearest to self.
#[must_use]
pub fn neighboring_leaderboard_entry_refs(
    entries: &[LeaderboardEntry],
    max_rows: usize,
) -> PrioritizedLeaderboardEntryRefs<'_> {
    prioritized_leaderboard_entry_refs_with_neighbors(entries, max_rows, true)
}

#[must_use]
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
