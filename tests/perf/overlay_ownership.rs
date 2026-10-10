use super::*;
use crate::overlay_support::compare;
use crate::perf::measure;
use crate::views::{SimplyLoveReleaseAssetView, SimplyLoveReleaseView};
use std::hint::black_box;

fn release() -> SimplyLoveReleaseView {
    SimplyLoveReleaseView {
        tag: "v0.5.1871".into(),
        html_url: "https://example.test/releases/é好🙂".repeat(4),
        published_at: Some("2026-10-10T12:34:56Z".into()),
    }
}

fn phases() -> Vec<ActionPhase> {
    let info = release();
    let mut out = vec![
        ActionPhase::Idle,
        ActionPhase::Checking,
        ActionPhase::RollbackChecking,
        ActionPhase::RollbackEmpty,
        ActionPhase::Ready { info: info.clone() },
        ActionPhase::Applying { info: info.clone() },
        ActionPhase::AvailableNoInstall { info: info.clone() },
        ActionPhase::RollbackPick {
            candidates: vec![],
            selected: usize::MAX,
        },
    ];
    for tag in ["", "v1.2.3", "v01234567890123456789é好🙂"] {
        out.push(ActionPhase::UpToDate { tag: tag.into() });
    }
    for selected in [0, 1, 20] {
        out.push(ActionPhase::RollbackPick {
            candidates: vec![
                info.clone(),
                SimplyLoveReleaseView {
                    published_at: None,
                    ..info.clone()
                },
            ],
            selected,
        });
    }
    for digest in [
        None,
        Some("  sha256:AbCdEF0123456789  ".into()),
        Some(" ".into()),
    ] {
        for published_at in [
            None,
            Some("2026-10-10T12:34:56Z".into()),
            Some("invalid".into()),
        ] {
            out.push(ActionPhase::ConfirmDownload {
                info: SimplyLoveReleaseView {
                    published_at,
                    ..info.clone()
                },
                asset: SimplyLoveReleaseAssetView {
                    size: 123_456_789,
                    digest: digest.clone(),
                },
            });
        }
    }
    for total in [None, Some(0), Some(100)] {
        for written in [0, 50, 200, u64::MAX] {
            for eta_secs in [None, Some(0), Some(3601)] {
                out.push(ActionPhase::Downloading {
                    info: info.clone(),
                    written,
                    total,
                    eta_secs,
                });
            }
        }
    }
    for kind in [
        ActionErrorKind::Network,
        ActionErrorKind::RateLimited,
        ActionErrorKind::HttpStatus,
        ActionErrorKind::Parse,
        ActionErrorKind::NoAssetForHost,
        ActionErrorKind::Checksum,
        ActionErrorKind::Io,
    ] {
        for detail in [
            String::new(),
            "é好🙂".repeat(40),
            "x".repeat(80),
            "y".repeat(81),
        ] {
            out.push(ActionPhase::Error {
                kind,
                detail: detail.clone(),
            });
            out.push(ActionPhase::AppliedRestartRequired {
                info: info.clone(),
                detail,
            });
        }
    }
    out
}

#[test]
fn updater_retained_content_matches_original_all_phases() {
    crate::i18n::init_for_tests();
    for phase in phases() {
        assert_eq!(
            phase_strings(&phase),
            perf_original::phase_strings(&phase),
            "{phase:?}"
        );
        perf_original::assert_same(perf_original::prepare(&phase), prepare(&phase));
    }
}

#[test]
fn updater_preparation_reduces_allocation_churn() {
    crate::i18n::init_for_tests();
    for phase in [
        ActionPhase::Checking,
        ActionPhase::Ready { info: release() },
        ActionPhase::Downloading {
            info: release(),
            written: 1024,
            total: Some(8192),
            eta_secs: Some(4),
        },
    ] {
        drop(prepare(&phase));
        let (_, before) = measure(|| black_box(perf_original::prepare(black_box(&phase))));
        let (_, after) = measure(|| black_box(prepare(black_box(&phase))));
        assert!(
            after.allocs < before.allocs,
            "{phase:?}: {before:?} -> {after:?}"
        );
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
    crate::perf::assert_no_churn(|| {
        black_box(prepare(black_box(&ActionPhase::Idle)));
    });
}

#[test]
fn panel_storage_preserves_sharing_and_animation_frames() {
    let shared: Arc<str> = Arc::from("A translated title longer than fourteen bytes");
    let text = retained_text(shared.clone().into());
    let TextContent::Shared(retained) = text else {
        panic!("long translation must remain shared")
    };
    assert!(Arc::ptr_eq(&shared, &retained));
    for footer in [
        "",
        "OK",
        "Wait…",
        "Long translated waiting message…",
        "é好🙂…",
        "Already...",
        "…",
    ] {
        let (before, old_animated) = perf_original::footer_frames(footer.to_owned());
        let (after, animated) = footer_frames(Arc::<str>::from(footer).into());
        assert_eq!(old_animated, animated);
        for (old, new) in before.iter().zip(&after) {
            assert_eq!(old.as_str(), new.as_str());
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_overlay_ownership_updater() {
    crate::i18n::init_for_tests();
    for (label, phase) in [
        ("updater/idle", ActionPhase::Idle),
        ("updater/checking", ActionPhase::Checking),
        ("updater/ready", ActionPhase::Ready { info: release() }),
        (
            "updater/download",
            ActionPhase::Downloading {
                info: release(),
                written: 12_345_678,
                total: Some(98_765_432),
                eta_secs: Some(65),
            },
        ),
        (
            "updater/confirm",
            ActionPhase::ConfirmDownload {
                info: release(),
                asset: SimplyLoveReleaseAssetView {
                    size: 98_765_432,
                    digest: Some("sha256:abcd1234".into()),
                },
            },
        ),
    ] {
        compare(
            label,
            || {
                black_box(perf_original::prepare(black_box(&phase)));
            },
            || {
                black_box(prepare(black_box(&phase)));
            },
        );
    }
}

#[test]
fn public_string_helper_does_not_add_allocation_churn() {
    crate::i18n::init_for_tests();
    for phase in phases() {
        drop(phase_strings(&phase));
        let (_, before) = measure(|| black_box(perf_original::phase_strings(black_box(&phase))));
        let (_, after) = measure(|| black_box(phase_strings(black_box(&phase))));
        assert_eq!(before.allocs, after.allocs, "{phase:?}");
        assert_eq!(before.reallocs, after.reallocs, "{phase:?}");
        assert_eq!(before.allocated_bytes, after.allocated_bytes, "{phase:?}");
    }
}
