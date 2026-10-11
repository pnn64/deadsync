use super::*;
use crate::overlay_support::compare;
use crate::perf::measure;
use crate::screens::components::shared::update_overlay::perf_original::assert_same;
use std::hint::black_box;

fn phases() -> Vec<FfmpegPhase> {
    let mut out = vec![
        FfmpegPhase::Idle,
        FfmpegPhase::Checking,
        FfmpegPhase::Unsupported,
        FfmpegPhase::AlreadyAvailable,
    ];
    for version in ["", "  8.1.1  ", "V7.0", "v01234567890123456789é好🙂"] {
        out.push(FfmpegPhase::Extracting {
            version: version.into(),
        });
        out.push(FfmpegPhase::Installed {
            version: version.into(),
        });
        for total in [None, Some(0), Some(100)] {
            for already_available in [false, true] {
                out.push(FfmpegPhase::Confirm {
                    version: version.into(),
                    origin: "Local source é好🙂".into(),
                    total,
                    already_available,
                });
            }
            for written in [0, 50, 200, u64::MAX] {
                for eta_secs in [None, Some(0), Some(3601)] {
                    for speed_bps in [None, Some(0), Some(1_234_567)] {
                        out.push(FfmpegPhase::Downloading {
                            version: version.into(),
                            written,
                            total,
                            eta_secs,
                            speed_bps,
                        });
                    }
                }
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
            out.push(FfmpegPhase::Error { kind, detail });
        }
    }
    out
}

#[test]
fn ffmpeg_retained_content_matches_original_all_phases() {
    crate::i18n::init_for_tests();
    for phase in phases() {
        assert_same(perf_original::prepare(&phase), prepare(&phase));
    }
}

#[test]
fn ffmpeg_preparation_reduces_allocation_churn() {
    crate::i18n::init_for_tests();
    for phase in [
        FfmpegPhase::Checking,
        FfmpegPhase::Unsupported,
        FfmpegPhase::Downloading {
            version: "8.1.1".into(),
            written: 1024,
            total: Some(8192),
            eta_secs: Some(4),
            speed_bps: Some(256),
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
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_overlay_ownership_ffmpeg() {
    crate::i18n::init_for_tests();
    for (label, phase) in [
        ("ffmpeg/idle", FfmpegPhase::Idle),
        ("ffmpeg/checking", FfmpegPhase::Checking),
        ("ffmpeg/unsupported", FfmpegPhase::Unsupported),
        (
            "ffmpeg/download",
            FfmpegPhase::Downloading {
                version: "8.1.1".into(),
                written: 12_345_678,
                total: Some(98_765_432),
                eta_secs: Some(65),
                speed_bps: Some(123_456),
            },
        ),
        (
            "ffmpeg/confirm",
            FfmpegPhase::Confirm {
                version: "8.1.1".into(),
                origin: "gyan.dev".into(),
                total: Some(98_765_432),
                already_available: true,
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
