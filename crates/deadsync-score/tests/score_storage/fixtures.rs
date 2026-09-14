use super::*;
use deadsync_core::input::InputSource;
pub(super) fn score_entry(
    played_at_ms: i64,
    score_percent: f64,
    failed: bool,
    replay_len: usize,
) -> LocalScoreEntry {
    LocalScoreEntry {
        version: crate::LOCAL_SCORE_VERSION,
        played_at_ms,
        music_rate: 1.0,
        score_percent,
        grade_code: crate::grade_to_code(if failed { Grade::Failed } else { Grade::Tier03 }),
        lamp_index: Some(2),
        lamp_judge_count: Some(4),
        ex_score_percent: score_percent * 100.0,
        hard_ex_score_percent: score_percent * 100.0,
        judgment_counts: [100, 4, 3, 2, 1, 0],
        holds_held: 5,
        holds_total: 6,
        rolls_held: 7,
        rolls_total: 8,
        mines_avoided: 9,
        mines_total: 10,
        hands_achieved: 11,
        fail_time: failed.then_some(42.0),
        beat0_time_ns: -250_000_000,
        replay: (0..replay_len)
            .map(|index| {
                crate::LocalReplayEdge::new(
                    1_000_000_000 + index as i64 * 8_000_000,
                    (index % 8) as u8,
                    index.is_multiple_of(2),
                    if index.is_multiple_of(3) {
                        InputSource::Gamepad
                    } else {
                        InputSource::Keyboard
                    },
                )
            })
            .collect(),
    }
}

pub(super) fn write_score(
    dir: &Path,
    chart_hash: &str,
    played_at_ms: i64,
    score_percent: f64,
    failed: bool,
    replay_len: usize,
) -> PathBuf {
    fs::create_dir_all(dir).expect("score directory should be creatable");
    let path = dir.join(format!("{chart_hash}-{played_at_ms}.bin"));
    let bytes = encode_local_score_entry(&score_entry(
        played_at_ms,
        score_percent,
        failed,
        replay_len,
    ))
    .expect("score fixture should encode");
    fs::write(&path, bytes).expect("score fixture should be writable");
    path
}
