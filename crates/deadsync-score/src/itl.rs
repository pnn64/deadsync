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
    if data.path_map.is_empty() && data.hash_map.is_empty() && data.unlock_folders.is_empty() {
        return None;
    }
    Some(data)
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
