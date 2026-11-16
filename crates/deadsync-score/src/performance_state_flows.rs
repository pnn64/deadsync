use super::*;

fn score(percent: f64, failed: bool, lamp: Option<u8>) -> CachedScore {
    CachedScore {
        grade: if failed { Grade::Failed } else { Grade::Tier01 },
        score_percent: percent,
        lamp_index: lamp,
        lamp_judge_count: Some(2),
    }
}


#[test]
fn fixed_width_snapshot_sort_matches_string_order_for_all_byte_ranges() {
    let mut hashes: Vec<_> = (1..257_u64)
        .map(|index| {
            format!(
                "{:016x}",
                index.wrapping_mul(0x9e3779b97f4a7c15).rotate_left(23)
            )
        })
        .collect();
    hashes.extend([
        "0000000000000000".into(),
        "FFFFFFFFFFFFFFFF".into(),
        "ffffffffffffffff".into(),
        "\0".repeat(16),
        "\u{e9}".repeat(8),
        "\u{6771}\u{4eac}0123456789".into(),
    ]);
    for variable_width in [false, true] {
        let mut actual: Vec<_> = hashes
            .iter()
            .enumerate()
            .map(|(index, hash)| (hash.clone(), score(index as f64, false, None)))
            .collect();
        if variable_width {
            actual.push(("short".into(), score(1.0, true, None)));
        }
        let mut expected = actual.clone();
        expected.sort_unstable_by(|(a, _), (b, _)| a.cmp(b));
        sort_profile_scores(&mut actual);
        assert_eq!(actual, expected);
        crate::perf::assert_no_churn(|| {
            sort_profile_scores(&mut actual);
        });
    }
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RetryItem<'a> {
    hash: &'a str,
    value: u32,
}


#[test]
fn borrowed_retry_key_has_no_churn_on_update_or_reserved_insert() {
    let mut state = SubmitRetryState::default();
    state.entries_mut(0).reserve(4);
    for (value, hash) in ["0123456789abcdef", "0123456789ABCDEF", "another-chart"]
        .into_iter()
        .enumerate()
    {
        crate::perf::assert_no_churn(|| {
            state.upsert_by_key(
                0,
                RetryItem {
                    hash,
                    value: value as u32,
                },
                |entry| entry.hash,
                4,
            );
        });
    }
    assert_eq!(state.entries(0).len(), 2);
    assert_eq!(state.entries(0)[0].value, 1);
}
