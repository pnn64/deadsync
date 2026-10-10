use super::*;
use crate::perf;
use std::hint::black_box;

mod original {
    include!("user_context_original.rs");
}
#[allow(dead_code)]
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn user(count: usize, padded: bool) -> ArrowCloudUserApiUser {
    let pad = if padded { " \t\u{2003}" } else { "" };
    ArrowCloudUserApiUser {
        id: format!("{pad}self-user-00000000{pad}"),
        rival_user_ids: (0..count)
            .map(|i| format!("{pad}rival-user-{i:08}{pad}"))
            .collect(),
    }
}

#[test]
fn owned_user_ids_preserve_unicode_whitespace_case_and_duplicate_semantics() {
    let values = [
        "",
        "  ",
        "\t\r\n\u{2003}",
        "rival",
        " rival ",
        "RIVAL",
        "\u{2003}caf\u{e9}\u{a0}",
        "a b",
        "\0id",
    ];
    for id in values {
        let make = || ArrowCloudUserApiUser {
            id: id.into(),
            rival_user_ids: values
                .iter()
                .chain(values.iter())
                .map(|s| (*s).into())
                .collect(),
        };
        assert_eq!(
            user_context_from_api(make()),
            original::user_context_from_api(make())
        );
    }
    let result = user_context_from_api(ArrowCloudUserApiUser {
        id: " \t ".into(),
        rival_user_ids: vec![" a ".into(), "a".into(), "A".into(), " ".into()],
    });
    assert!(result.self_user_id.is_none());
    assert_eq!(
        result.rival_user_ids,
        HashSet::from(["a".into(), "A".into()])
    );
}

#[test]
fn owned_user_context_reuses_string_allocations() {
    for padded in [false, true] {
        let before = user(64, padded);
        let after = user(64, padded);
        let (expected, old) = perf::measure(|| original::user_context_from_api(before));
        let (actual, new) = perf::measure(|| user_context_from_api(after));
        assert_eq!(actual, expected);
        assert_eq!(old.allocs - new.allocs, 65);
        assert!(new.allocated_bytes < old.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_user_context() {
    for count in [0, 8, 64, 512] {
        for padded in [false, true] {
            let label = format!("user-context rivals={count} padded={padded}");
            for current in [false, true] {
                let input = user(count, padded);
                let (result, churn) = perf::measure(|| {
                    if current {
                        user_context_from_api(input)
                    } else {
                        original::user_context_from_api(input)
                    }
                });
                black_box(result);
                println!("{label} current={current}: {churn:?}");
            }
            paired::compare_prepared(
                &label,
                if count == 512 { 256 } else { 2048 },
                || user(count, padded),
                |input, current| {
                    black_box(if current {
                        user_context_from_api(black_box(input))
                    } else {
                        original::user_context_from_api(black_box(input))
                    });
                },
            );
        }
    }
}
