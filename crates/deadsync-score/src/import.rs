//! Pure conversion from an ITGmania `Stats.xml` `<HighScore>` record into a
//! DeadSync [`LocalScoreEntry`].
//!
//! This module is intentionally engine-free and side-effect-free so it can be
//! unit-tested in isolation. The XML/INI parsing and the on-disk writing live
//! in the main `deadsync` crate; this layer only performs the field mapping.
//!
//! ITGmania does not record the FA+ (W0) blue/white split, so neither the EX
//! nor the Hard-EX score can be reconstructed from `Stats.xml`. Both are stored
//! as `0.0`; only the ITG percent and grade carry over.

use crate::{Grade, LOCAL_SCORE_VERSION, LocalScoreEntry, compute_local_lamp, grade_to_code};
use chrono::{NaiveDateTime, TimeZone};

/// A single ITGmania high-score record, parsed from `Stats.xml`.
///
/// Field names mirror the ITGmania `HighScore` XML schema (see
/// `itgmania/src/HighScore.cpp`). All tap/hold tallies default to `0` when the
/// corresponding XML element is absent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportedHighScore {
    /// Raw `<Grade>` text, e.g. `"Grade_Tier01"` or `"Grade_Failed"`.
    pub grade: String,
    /// `<PercentDP>` as a 0.0–1.0 ratio (ITGmania stores it pre-divided).
    pub percent_dp: f64,
    /// `<DateTime>` text, expected as `"YYYY-MM-DD HH:MM:SS"` (local time).
    pub date_time: String,
    /// `<TapNoteScores>` tallies.
    pub w1: u32,
    pub w2: u32,
    pub w3: u32,
    pub w4: u32,
    pub w5: u32,
    pub miss: u32,
    pub hit_mine: u32,
    pub avoid_mine: u32,
    /// `<HoldNoteScores>` tallies.
    pub held: u32,
    pub let_go: u32,
    pub missed_hold: u32,
    /// `<SurviveSeconds>` — how long the player lasted before failing (if any).
    pub survive_seconds: f32,
    /// `<Modifiers>` text, e.g. `"1.5xMusic, Overhead"`. Used to recover the
    /// music rate; empty/absent means the default rate of 1.0.
    pub modifiers: String,
}

/// Recovers the music rate from an ITGmania `<Modifiers>` string.
///
/// ITGmania serialises a non-default rate as a `"<rate>xMusic"` token (see
/// `itgmania/src/SongOptions.cpp` `GetMods`), e.g. `"1.5xMusic"`. The token is
/// comma/space separated from the other modifiers. Returns `1.0` when no rate
/// token is present or it can't be parsed into a positive number.
pub fn music_rate_from_modifiers(modifiers: &str) -> f32 {
    for token in modifiers.split([',', ' ', '\t']) {
        let token = token.trim();
        if token.len() < 7 {
            continue;
        }
        let (num, suffix) = token.split_at(token.len() - 6);
        if !suffix.eq_ignore_ascii_case("xmusic") {
            continue;
        }
        if let Ok(rate) = num.parse::<f32>()
            && rate.is_finite() && rate > 0.0 {
                return rate;
            }
    }
    1.0
}

/// Maps an ITGmania `<Grade>` string to a DeadSync [`Grade`].
///
/// ITGmania's `Stats.xml` stores the grade as the bare tier name (e.g.
/// `"Tier04"`, `"Failed"`), whereas the enum form used elsewhere carries a
/// `"Grade_"` prefix (e.g. `"Grade_Tier04"`). Both are accepted.
///
/// Returns `None` for unrecognized values so the caller can decide how to treat
/// them (DeadSync has no `Grade_Tier00`/`Grade_Quint` analogue in ITGmania).
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

/// Parses an ITGmania `<DateTime>` (`"YYYY-MM-DD HH:MM:SS"`, local time) into
/// epoch milliseconds. Returns `None` if the string can't be parsed.
pub fn parse_itg_datetime_ms(date_time: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(date_time.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    // ITGmania writes timestamps in machine-local time. Interpret them the same
    // way; if the local offset is ambiguous (DST fold) take the earliest.
    match chrono::Local.from_local_datetime(&naive) {
        chrono::offset::LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        chrono::offset::LocalResult::Ambiguous(dt, _) => Some(dt.timestamp_millis()),
        chrono::offset::LocalResult::None => None,
    }
}

/// Converts an [`ImportedHighScore`] into a DeadSync [`LocalScoreEntry`].
///
/// Returns `None` when the grade is unrecognized (the play can't be imported
/// without a valid DeadSync grade).
///
/// Mapping notes:
/// * `judgment_counts = [W1, W2, W3, W4, W5, Miss]`.
/// * Holds and rolls are not distinguished in `Stats.xml`; all hold-type tallies
///   are folded into the hold fields (`rolls_* = 0`).
/// * `score_percent` is taken from `PercentDP` directly (both are 0.0–1.0).
/// * `music_rate` is recovered from the `<Modifiers>` rate token (default 1.0).
/// * EX / Hard-EX are unrecoverable → `0.0`.
/// * The lamp is recomputed from the judgment counts (W0 split unknown).
pub fn local_score_from_itg(hs: &ImportedHighScore) -> Option<LocalScoreEntry> {
    let grade = grade_from_itg(&hs.grade)?;

    let counts = [hs.w1, hs.w2, hs.w3, hs.w4, hs.w5, hs.miss];
    let holds_total = hs
        .held
        .saturating_add(hs.let_go)
        .saturating_add(hs.missed_hold);
    let mines_total = hs.hit_mine.saturating_add(hs.avoid_mine);

    let (lamp_index, lamp_judge_count) = compute_local_lamp(counts, grade, None);

    let fail_time = if grade == Grade::Failed {
        Some(hs.survive_seconds.max(0.0))
    } else {
        None
    };

    Some(LocalScoreEntry {
        version: LOCAL_SCORE_VERSION,
        played_at_ms: parse_itg_datetime_ms(&hs.date_time).unwrap_or(0),
        music_rate: music_rate_from_modifiers(&hs.modifiers),
        score_percent: hs.percent_dp.clamp(0.0, 1.0),
        grade_code: grade_to_code(grade),
        lamp_index,
        lamp_judge_count,
        ex_score_percent: 0.0,
        hard_ex_score_percent: 0.0,
        judgment_counts: counts,
        holds_held: hs.held,
        holds_total,
        rolls_held: 0,
        rolls_total: 0,
        mines_avoided: hs.avoid_mine,
        mines_total,
        hands_achieved: 0,
        fail_time,
        beat0_time_ns: 0,
        replay: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_grades() {
        // Prefixed enum form (used by some tooling / older exports).
        assert_eq!(grade_from_itg("Grade_Tier01"), Some(Grade::Tier01));
        assert_eq!(grade_from_itg("Grade_Tier17"), Some(Grade::Tier17));
        assert_eq!(grade_from_itg("Grade_Failed"), Some(Grade::Failed));
        assert_eq!(grade_from_itg("  Grade_Tier04  "), Some(Grade::Tier04));
        // Bare tier form actually written by ITGmania's Stats.xml.
        assert_eq!(grade_from_itg("Tier01"), Some(Grade::Tier01));
        assert_eq!(grade_from_itg("Tier04"), Some(Grade::Tier04));
        assert_eq!(grade_from_itg("Tier17"), Some(Grade::Tier17));
        assert_eq!(grade_from_itg("Failed"), Some(Grade::Failed));
        assert_eq!(grade_from_itg("  Tier05  "), Some(Grade::Tier05));
        // Unknown tiers and junk are rejected in both forms.
        assert_eq!(grade_from_itg("Grade_Tier00"), None);
        assert_eq!(grade_from_itg("Tier00"), None);
        assert_eq!(grade_from_itg("nonsense"), None);
    }

    #[test]
    fn parses_datetime_round_numbers() {
        // Round-trip through local time: parse then reformat must match.
        let ms = parse_itg_datetime_ms("2023-04-15 21:07:33").expect("parse");
        let dt = chrono::Local
            .timestamp_millis_opt(ms)
            .single()
            .expect("local");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2023-04-15 21:07:33"
        );
        assert_eq!(parse_itg_datetime_ms("not a date"), None);
    }

    #[test]
    fn rejects_unknown_grade() {
        let hs = ImportedHighScore {
            grade: "Grade_Bogus".into(),
            ..Default::default()
        };
        assert!(local_score_from_itg(&hs).is_none());
    }

    #[test]
    fn maps_a_passing_quad_star() {
        let hs = ImportedHighScore {
            grade: "Grade_Tier01".into(),
            percent_dp: 0.9912,
            date_time: "2023-04-15 21:07:33".into(),
            w1: 480,
            w2: 12,
            w3: 0,
            w4: 0,
            w5: 0,
            miss: 0,
            hit_mine: 0,
            avoid_mine: 3,
            held: 20,
            let_go: 0,
            missed_hold: 0,
            survive_seconds: 0.0,
            modifiers: String::new(),
        };
        let e = local_score_from_itg(&hs).expect("entry");
        assert_eq!(e.grade_code, grade_to_code(Grade::Tier01));
        assert_eq!(e.judgment_counts, [480, 12, 0, 0, 0, 0]);
        assert_eq!(e.holds_held, 20);
        assert_eq!(e.holds_total, 20);
        assert_eq!(e.rolls_total, 0);
        assert_eq!(e.mines_avoided, 3);
        assert_eq!(e.mines_total, 3);
        assert!((e.score_percent - 0.9912).abs() < 1e-9);
        assert_eq!(e.ex_score_percent, 0.0);
        assert_eq!(e.fail_time, None);
        assert_eq!(e.music_rate, 1.0);
        // No misses/way-offs/etc beyond excellents → "excellent" lamp tier.
        assert_eq!(e.lamp_index, Some(2));
        assert_eq!(e.lamp_judge_count, None); // 12 excellents > 9 → no judge count
    }

    #[test]
    fn maps_a_failed_run() {
        let hs = ImportedHighScore {
            grade: "Grade_Failed".into(),
            percent_dp: 0.4231,
            date_time: "2022-01-02 03:04:05".into(),
            w1: 100,
            miss: 40,
            survive_seconds: 51.5,
            ..Default::default()
        };
        let e = local_score_from_itg(&hs).expect("entry");
        assert_eq!(e.grade_code, grade_to_code(Grade::Failed));
        assert_eq!(e.fail_time, Some(51.5));
        assert_eq!(e.lamp_index, None);
    }

    #[test]
    fn folds_let_go_and_missed_into_hold_total() {
        let hs = ImportedHighScore {
            grade: "Grade_Tier05".into(),
            held: 10,
            let_go: 3,
            missed_hold: 2,
            ..Default::default()
        };
        let e = local_score_from_itg(&hs).expect("entry");
        assert_eq!(e.holds_held, 10);
        assert_eq!(e.holds_total, 15);
    }

    #[test]
    fn parses_music_rate_from_modifiers() {
        assert_eq!(music_rate_from_modifiers(""), 1.0);
        assert_eq!(music_rate_from_modifiers("Overhead, Mirror"), 1.0);
        assert_eq!(music_rate_from_modifiers("1.5xMusic"), 1.5);
        assert_eq!(
            music_rate_from_modifiers("Overhead, 2.0xMusic, Mirror"),
            2.0
        );
        assert_eq!(music_rate_from_modifiers("0.8xmusic"), 0.8);
        // Malformed / non-positive rate falls back to 1.0.
        assert_eq!(music_rate_from_modifiers("xMusic"), 1.0);
        assert_eq!(music_rate_from_modifiers("0xMusic"), 1.0);
    }

    #[test]
    fn applies_music_rate_to_entry() {
        let hs = ImportedHighScore {
            grade: "Grade_Tier03".into(),
            modifiers: "1.5xMusic, Reverse".into(),
            ..Default::default()
        };
        let e = local_score_from_itg(&hs).expect("entry");
        assert_eq!(e.music_rate, 1.5);
    }
}
