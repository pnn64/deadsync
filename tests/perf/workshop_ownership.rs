use super::*;
use crate::overlay_support::compare;
use crate::perf::measure;
use crate::screens::components::shared::update_overlay::perf_original::assert_same;
use std::hint::black_box;

#[test]
fn workshop_retained_content_matches_original_all_phases() {
    crate::i18n::init_for_tests();
    let mut phases = vec![
        Phase::Idle,
        Phase::Publishing,
        Phase::Cancelling,
        Phase::Installed,
    ];
    for total in [0, 100, u64::MAX] {
        for written in [0, 50, 200, u64::MAX] {
            phases.push(Phase::Downloading { written, total });
            phases.push(Phase::Preparing {
                done: written as usize,
                total: total as usize,
            });
        }
    }
    for detail in [
        String::new(),
        "é好🙂".repeat(60),
        "x".repeat(99),
        "y".repeat(100),
        "z".repeat(101),
    ] {
        phases.push(Phase::Error { detail });
    }
    for phase in phases {
        assert_same(
            perf_original::prepare_workshop(&phase),
            prepare_workshop(&phase),
        );
    }
}

#[test]
fn workshop_preparation_reduces_allocation_churn() {
    crate::i18n::init_for_tests();
    for phase in [
        Phase::Preparing {
            done: 4,
            total: 100,
        },
        Phase::Installed,
        Phase::Downloading {
            written: 1024,
            total: 8192,
        },
    ] {
        drop(prepare_workshop(&phase));
        let (_, before) = measure(|| black_box(perf_original::prepare_workshop(black_box(&phase))));
        let (_, after) = measure(|| black_box(prepare_workshop(black_box(&phase))));
        assert!(
            after.allocs < before.allocs,
            "{phase:?}: {before:?} -> {after:?}"
        );
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_overlay_ownership_workshop() {
    crate::i18n::init_for_tests();
    for (label, phase) in [
        ("workshop/idle", Phase::Idle),
        (
            "workshop/preparing",
            Phase::Preparing {
                done: 123,
                total: 987,
            },
        ),
        ("workshop/installed", Phase::Installed),
        (
            "workshop/download",
            Phase::Downloading {
                written: 12_345_678,
                total: 98_765_432,
            },
        ),
        (
            "workshop/error",
            Phase::Error {
                detail: "Error: é好🙂".repeat(30),
            },
        ),
    ] {
        compare(
            label,
            || {
                black_box(perf_original::prepare_workshop(black_box(&phase)));
            },
            || {
                black_box(prepare_workshop(black_box(&phase)));
            },
        );
    }
}
