//! Exact readout behavior and paired performance comparisons with 0.5.1213.
use deadsync_theme::views::FrameStatsSummary;
use std::hint::black_box;
use std::sync::Arc;

mod act_macro {
    macro_rules! act {
        (quad: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+) ::deadsync_assets::present_dsl::SpriteBuilder::solid()
            )
        }};
        (text: $($tail:tt)+) => {{
            ::deadlib_present::__act_from_builder!(
                ($($tail)+) ::deadsync_assets::present_dsl::TextBuilder::new()
            )
        }};
    }
    pub(crate) use act;
}
pub(crate) use act_macro::act;
mod views {
    pub use deadsync_theme_simply_love::views::{HISTOGRAM_BINS, frame_histogram};
}
#[allow(dead_code, unused_imports)]
#[path = "frame_diagnostics/readout_baseline.rs"]
mod baseline;
#[allow(dead_code)]
#[path = "../../../tests/support/perf.rs"]
mod perf;

#[allow(dead_code)]
mod current {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/screens/components/shared/frame_stats_overlay.rs"
    ));

    pub(super) fn readouts(s: &FrameStatsSummary, p99: bool) -> [Arc<str>; 6] {
        [
            retained_summary_text(s, p99),
            retained_load_text(s),
            retained_stutter_text(s),
            retained_display_text(s, p99),
            retained_audio_text(s),
            retained_compact_text(s, p99),
        ]
    }

    pub(super) fn reset() {
        SUMMARY_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
        LOAD_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
        STUTTER_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
        DISPLAY_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
        AUDIO_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
        COMPACT_TEXT_CACHE.with(|c| *c.borrow_mut() = ReadoutCache::new());
    }

    pub(super) fn storage() -> (usize, usize) {
        let mut entries = 0;
        let mut bytes = 0;
        SUMMARY_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        LOAD_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        STUTTER_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        DISPLAY_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        AUDIO_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        COMPACT_TEXT_CACHE.with(|c| {
            let c = c.borrow();
            entries += c.entries.len();
            bytes += c.entries.values().map(|s| s.len()).sum::<usize>();
        });
        (entries, bytes)
    }

    #[test]
    fn saturated_cache_reuses_text_without_growing_and_keeps_live_arcs_immutable() {
        let mut cache = ReadoutCache::new();
        let held = cache.get(0, |s| s.push_str("original"));
        for i in 1..2048 {
            cache.get(i, |s| {
                use std::fmt::Write;
                write!(s, "reading {i}").unwrap();
            });
        }
        assert_eq!(cache.entries.len(), TEXT_CACHE_LIMIT);
        let last = cache.get(3000, |s| s.push_str("last"));
        crate::perf::assert_no_churn(|| {
            let same = cache.get(3001, |s| s.push_str("last"));
            assert!(Arc::ptr_eq(&last, &same));
            let hit = cache.get(3001, |_| panic!("unchanged keys must not format"));
            assert!(Arc::ptr_eq(&last, &hit));
            let alias = cache.get(3000, |_| {
                panic!("equivalent alternating keys must not format")
            });
            assert!(Arc::ptr_eq(&last, &alias));
        });
        assert_eq!(&*cache.get(3002, |s| s.push_str("changed")), "changed");
        assert_eq!(&*cache.get(3000, |s| s.push_str("last")), "last");
        assert_eq!(&*held, "original");
        assert!(Arc::ptr_eq(
            &held,
            &cache.get(0, |_| panic!("retained key must hit"))
        ));
    }
}

fn old_readouts(s: &FrameStatsSummary, p99: bool) -> [Arc<str>; 6] {
    [
        baseline::retained_summary_text(s, p99),
        baseline::retained_load_text(s),
        baseline::retained_stutter_text(s),
        baseline::retained_display_text(s, p99),
        baseline::retained_audio_text(s),
        baseline::retained_compact_text(s, p99),
    ]
}

fn summary() -> FrameStatsSummary {
    FrameStatsSummary {
        avg_frame_us: 16_600,
        p99_frame_us: 22_000,
        max_frame_us: 30_000,
        fps: 60.0,
        display_error_ms: 0.4,
        display_error_p99_ms: 1.2,
        display_catching_up: false,
        in_gameplay: true,
        audio_callback_gap_ms: 2.0,
        audio_underruns: 0,
        audio_output_delay_ms: 12.0,
        audio_queued_frames: 512,
        frame_jitter_us: 300,
        display_error_jitter_us: 100,
        spike_hold_us: 30_000,
        target_frame_us: 16_667,
        cpu_work_us: 2000,
        gpu_wait_us: 800,
        over_budget_count: 3,
        catch_up_count: 1,
    }
}

fn jitter(i: u32) -> FrameStatsSummary {
    let mut s = summary();
    s.fps = f32::from_bits(s.fps.to_bits() + i % 4096);
    s.display_error_ms = f32::from_bits(s.display_error_ms.to_bits() + i % 4096);
    s.audio_callback_gap_ms = f32::from_bits(s.audio_callback_gap_ms.to_bits() + i % 4096);
    s.audio_output_delay_ms = f32::from_bits(s.audio_output_delay_ms.to_bits() + i % 4096);
    s
}

fn changing(i: u32) -> FrameStatsSummary {
    let mut s = summary();
    s.fps = i as f32 + 1.0;
    s.avg_frame_us = i * 10;
    s.spike_hold_us = i * 100;
    s.display_error_ms = i as f32;
    s.audio_underruns = u64::from(i);
    s
}

#[test]
fn every_readout_matches_parent_for_rounding_modes_and_extreme_values() {
    current::reset();
    baseline::reset();
    let mut values = vec![
        0.0,
        -0.0,
        f32::MIN,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ];
    for x in [0.005f32, 0.995, 9.995, 99.995, -0.005, -0.995, -9.995] {
        values.extend([
            f32::from_bits(x.to_bits() - 1),
            x,
            f32::from_bits(x.to_bits() + 1),
        ]);
    }
    for value in values {
        for p99 in [false, true] {
            for gameplay in [false, true] {
                let mut s = summary();
                s.in_gameplay = gameplay;
                s.fps = value;
                s.display_error_ms = value;
                s.display_error_p99_ms = value;
                s.audio_callback_gap_ms = value;
                s.audio_output_delay_ms = value;
                s.audio_underruns = u64::MAX;
                s.audio_queued_frames = u32::MAX;
                s.avg_frame_us = u32::MAX;
                s.frame_jitter_us = u32::MAX;
                s.display_error_jitter_us = u32::MAX;
                s.target_frame_us = 0;
                assert_eq!(current::readouts(&s, p99), old_readouts(&s, p99));
            }
        }
    }
    for i in 0..4096 {
        let s = changing(i);
        assert_eq!(
            current::readouts(&s, i % 2 == 0),
            old_readouts(&s, i % 2 == 0)
        );
    }
}

#[test]
fn rounded_telemetry_changes_reuse_strings_without_heap_churn() {
    current::reset();
    let expected = current::readouts(&jitter(0), true);
    perf::assert_no_churn(|| {
        for i in 1..4096 {
            let actual = current::readouts(&jitter(i), true);
            assert_eq!(actual, expected);
            for (a, b) in actual.iter().zip(&expected) {
                assert!(Arc::ptr_eq(a, b));
            }
        }
    });
}

#[test]
fn rounded_changes_avoid_retaining_duplicate_readout_payloads() {
    current::reset();
    baseline::reset();
    for i in 0..4096 {
        assert_eq!(
            current::readouts(&jitter(i), true),
            old_readouts(&jitter(i), true)
        );
    }
    let (old_entries, old_bytes) = baseline::storage();
    let (new_entries, new_bytes) = current::storage();
    assert!(new_entries < old_entries / 100);
    assert!(new_bytes < old_bytes / 100);
    eprintln!(
        "readout storage after 4096 rounded changes: entries {old_entries} -> {new_entries}, payload bytes {old_bytes} -> {new_bytes}"
    );
}

#[test]
fn changed_readouts_reduce_churn_after_cache_saturation() {
    current::reset();
    baseline::reset();
    for i in 0..2048 {
        black_box(old_readouts(&changing(i), true));
        black_box(current::readouts(&changing(i), true));
    }
    perf::assert_reduced_churn(
        || {
            black_box(old_readouts(&changing(10_000), true));
        },
        || {
            black_box(current::readouts(&changing(10_000), true));
        },
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn benchmark_frame_readouts() {
    for shape in [
        "stable",
        "alternating",
        "jitter",
        "changing",
        "saturated_stable",
    ] {
        current::reset();
        baseline::reset();
        if shape != "stable" && shape != "alternating" {
            for i in 0..2048 {
                black_box(old_readouts(&changing(i), true));
                black_box(current::readouts(&changing(i), true));
            }
        }
        let fixtures: Vec<_> = (0..4096)
            .map(|i| match shape {
                "stable" => summary(),
                "alternating" => changing(i % 2),
                "jitter" => jitter(i),
                "saturated_stable" => changing(10_000),
                _ => changing(i + 10_000),
            })
            .collect();
        let mut ai = 0;
        let mut bi = 0;
        let mut old = || {
            perf::measure_sampled(&format!("readout_{shape}/old"), 4096, 6, || {
                let s = &fixtures[ai % fixtures.len()];
                ai += 1;
                old_readouts(black_box(s), true)
            })
        };
        let mut new = || {
            perf::measure_sampled(&format!("readout_{shape}/new"), 4096, 6, || {
                let s = &fixtures[bi % fixtures.len()];
                bi += 1;
                current::readouts(black_box(s), true)
            })
        };
        if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
            new();
            old();
        } else {
            old();
            new();
        }
    }
}
