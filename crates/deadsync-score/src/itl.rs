use crate::CachedItlScore;
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

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
