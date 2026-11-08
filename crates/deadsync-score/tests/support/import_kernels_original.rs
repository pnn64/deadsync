// Frozen production kernels from 3fe56b047 for differential checks.
use deadsync_score::Grade;
use std::collections::HashSet;

pub fn collect_chart_hashes_per_pack_for_import(
    song_packs: &[deadsync_chart::SongPack],
    pack_groups_filter: &[String],
    existing_scores: &HashSet<String>,
) -> Vec<(String, Vec<String>)> {
    let mut nonempty_filters = pack_groups_filter
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    // Importing one selected pack needs neither a hash table nor owned names.
    let single_filter = nonempty_filters
        .next()
        .filter(|_| nonempty_filters.next().is_none());
    let filter_set: HashSet<String> = if single_filter.is_some() {
        HashSet::new()
    } else {
        pack_groups_filter
            .iter()
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .collect()
    };

    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut name_scratch = String::with_capacity(if filter_set.is_empty() { 0 } else { 64 });
    for pack in song_packs {
        let group_name = pack.group_name.trim();
        let display_name = if pack.name.trim().is_empty() {
            group_name
        } else {
            pack.name.trim()
        };
        if let Some(filter) = single_filter {
            if !group_name.eq_ignore_ascii_case(filter)
                && !display_name.eq_ignore_ascii_case(filter)
            {
                continue;
            }
        } else if !filter_set.is_empty() {
            // Reuse one lowercase buffer across the whole library, and avoid
            // normalizing the display alias when the group already matches.
            name_scratch.clear();
            name_scratch.push_str(group_name);
            name_scratch.make_ascii_lowercase();
            if !filter_set.contains(&name_scratch) {
                if display_name == group_name {
                    continue;
                }
                name_scratch.clear();
                name_scratch.push_str(display_name);
                name_scratch.make_ascii_lowercase();
                if !filter_set.contains(&name_scratch) {
                    continue;
                }
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

pub fn grade_from_itg(grade: &str) -> Option<Grade> {
    let trimmed = grade.trim();
    let tier = trimmed.strip_prefix("Grade_").unwrap_or(trimmed);
    match tier {
        "Tier01" => Some(Grade::Tier01),
        "Tier02" => Some(Grade::Tier02),
        "Tier03" => Some(Grade::Tier03),
        "Tier04" => Some(Grade::Tier04),
        "Tier05" => Some(Grade::Tier05),
        "Tier06" => Some(Grade::Tier06),
        "Tier07" => Some(Grade::Tier07),
        "Tier08" => Some(Grade::Tier08),
        "Tier09" => Some(Grade::Tier09),
        "Tier10" => Some(Grade::Tier10),
        "Tier11" => Some(Grade::Tier11),
        "Tier12" => Some(Grade::Tier12),
        "Tier13" => Some(Grade::Tier13),
        "Tier14" => Some(Grade::Tier14),
        "Tier15" => Some(Grade::Tier15),
        "Tier16" => Some(Grade::Tier16),
        "Tier17" => Some(Grade::Tier17),
        "Failed" => Some(Grade::Failed),
        _ => None,
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
