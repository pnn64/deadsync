use super::*;
use crate::perf;
use std::hint::black_box;

#[allow(unused_imports)]
mod original {
    include!("score_import_original.rs");
}
#[allow(dead_code)]
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

fn profile(valid: bool) -> Profile {
    let mut profile = Profile::default();
    profile.groovestats_api_key = "test-api-key".into();
    if valid {
        profile.groovestats_username = "Player".into();
    }
    profile
}

fn input(valid: bool) -> (String, Profile, Vec<String>) {
    (
        "profile-identifier".into(),
        profile(valid),
        vec!["__perf1866_no_matching_pack__".into()],
    )
}

fn import(
    current: bool,
    input: (String, Profile, Vec<String>),
) -> Result<ScoreBulkImportSummary, Box<dyn Error + Send + Sync>> {
    if current {
        import_scores_for_profile_from_app_runtime(
            ScoreImportEndpoint::GrooveStats,
            input.0,
            input.1,
            input.2,
            false,
            |_| {},
            || true,
        )
    } else {
        original::import_scores_for_profile_from_app_runtime(
            ScoreImportEndpoint::GrooveStats,
            input.0,
            input.1,
            input.2,
            false,
            |_| {},
            || true,
        )
    }
}

#[test]
fn direct_import_preserves_validation_and_empty_selection_progress() {
    for endpoint in [
        ScoreImportEndpoint::GrooveStats,
        ScoreImportEndpoint::BoogieStats,
        ScoreImportEndpoint::ArrowCloud,
    ] {
        for valid in [false, true] {
            for cancel in [false, true] {
                let mut outcomes = Vec::new();
                for current in [false, true] {
                    let (id, mut profile, groups) = input(valid);
                    // ArrowCloud resolves user context before cancellation; keep it offline.
                    profile.arrowcloud_api_key.clear();
                    let mut events = Vec::new();
                    let on_progress =
                        |progress: ScoreImportProgress| events.push(format!("{progress:?}"));
                    let result = if current {
                        import_scores_for_profile_from_app_runtime(
                            endpoint,
                            id,
                            profile,
                            groups,
                            false,
                            on_progress,
                            || cancel,
                        )
                    } else {
                        original::import_scores_for_profile_from_app_runtime(
                            endpoint,
                            id,
                            profile,
                            groups,
                            false,
                            on_progress,
                            || cancel,
                        )
                    };
                    let result = result
                        .map(|mut summary| {
                            assert_eq!(summary.requested_charts, 0);
                            summary.elapsed_seconds = 0.0;
                            format!("{summary:?}")
                        })
                        .map_err(|error| error.to_string());
                    outcomes.push((result, events));
                }
                assert_eq!(
                    outcomes[0], outcomes[1],
                    "{endpoint:?} valid={valid} cancel={cancel}"
                );
            }
        }
    }
}

#[test]
fn direct_grade_validation_preserves_errors_without_network_requests() {
    for valid in [false, true] {
        for hash in ["", " \t "] {
            let original = original::fetch_and_store_grade_from_app_runtime(
                "profile".into(),
                profile(valid),
                hash.into(),
            )
            .unwrap_err()
            .to_string();
            let current = fetch_and_store_grade_from_app_runtime(
                "profile".into(),
                profile(valid),
                hash.into(),
            )
            .unwrap_err()
            .to_string();
            assert_eq!(original, current);
        }
    }
}

#[test]
fn direct_callbacks_remove_six_boxes_and_four_temporary_strings() {
    let _ = crate::runtime::active_groovestats_service();
    let _ = import(true, input(true));
    let before = input(true);
    let after = input(true);
    let (_, old) = perf::measure(|| import(false, before));
    let (_, new) = perf::measure(|| import(true, after));
    assert_eq!(old.allocs - new.allocs, 10);
    assert!(old.allocated_bytes > new.allocated_bytes);
    let before = (
        "profile-identifier".to_string(),
        profile(false),
        "chart-identifier".to_string(),
    );
    let after = (
        "profile-identifier".to_string(),
        profile(false),
        "chart-identifier".to_string(),
    );
    let (_, old) = perf::measure(|| {
        original::fetch_and_store_grade_from_app_runtime(before.0, before.1, before.2)
    });
    let (_, new) =
        perf::measure(|| fetch_and_store_grade_from_app_runtime(after.0, after.1, after.2));
    assert_eq!(old.allocs - new.allocs, 10);
    assert!(old.allocated_bytes > new.allocated_bytes);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_direct_score_callbacks() {
    let _ = crate::runtime::active_groovestats_service();
    for valid in [false, true] {
        let label = format!("score-import valid={valid}");
        for current in [false, true] {
            let input = input(valid);
            let (result, churn) = perf::measure(|| import(current, input));
            let _ = black_box(result);
            println!("{label} current={current}: {churn:?}");
        }
        paired::compare_prepared(
            &label,
            4096,
            || input(valid),
            |input, current| {
                let _ = black_box(import(current, black_box(input)));
            },
        );
    }
    for current in [false, true] {
        let input = (
            "profile-identifier".into(),
            profile(false),
            "chart-identifier".into(),
        );
        let (result, churn) = perf::measure(|| {
            if current {
                fetch_and_store_grade_from_app_runtime(input.0, input.1, input.2)
            } else {
                original::fetch_and_store_grade_from_app_runtime(input.0, input.1, input.2)
            }
        });
        let _ = black_box(result);
        println!("score-grade current={current}: {churn:?}");
    }
    paired::compare_prepared(
        "score-grade",
        4096,
        || {
            (
                "profile-identifier".into(),
                profile(false),
                "chart-identifier".into(),
            )
        },
        |input, current| {
            let _ = black_box(if current {
                fetch_and_store_grade_from_app_runtime(input.0, input.1, input.2)
            } else {
                original::fetch_and_store_grade_from_app_runtime(input.0, input.1, input.2)
            });
        },
    );
}
