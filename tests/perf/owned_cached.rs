use super::*;
use crate::owned_perf::compare;
use crate::perf::measure;
use std::hint::black_box;

mod before {
    use super::*;
    include!("owned_cached_original.rs");
}

fn analysis(size: usize, applied: bool) -> CachedAnalysis {
    CachedAnalysis {
        bias_ms: -4.5,
        confidence: 0.93,
        applied,
        plot: Some(CachedPlot {
            freq_rows: 2,
            digest_rows: 1,
            cols: size.saturating_sub(1), // Legacy curves can have a longer time axis.
            post_rows: 1,
            freq_domain: vec![0.25; size * 2],
            beat_digest: vec![0.5; size],
            post_kernel: vec![-0.25; size],
            times_ms: (0..size).map(|i| i as f64 - 1.0).collect(),
            convolution: vec![0.75; size],
            edge_discard: 1,
        }),
    }
}

#[test]
fn cached_views_match_for_legacy_missing_empty_and_applied_plots() {
    for size in [0, 1, 64, 4096] {
        for applied in [false, true] {
            let cached = analysis(size, applied);
            let expected = before::cached_song_result(&cached);
            assert_eq!(cached_song_result(cached), expected);
        }
    }
    for applied in [false, true] {
        let mut cached = analysis(0, applied);
        cached.plot = None;
        assert_eq!(
            cached_song_result(cached.clone()),
            before::cached_song_result(&cached)
        );
    }
}

#[test]
fn cached_views_move_all_five_buffers_without_allocating() {
    crate::perf::assert_no_churn(|| {
        black_box(cached_song_result(CachedAnalysis {
            bias_ms: 0.0,
            confidence: 1.0,
            applied: false,
            plot: None,
        }));
    });
    let cached = analysis(4096, false);
    let p = cached.plot.as_ref().unwrap();
    let pointers = [
        p.freq_domain.as_ptr(),
        p.beat_digest.as_ptr(),
        p.post_kernel.as_ptr(),
        p.times_ms.as_ptr(),
        p.convolution.as_ptr(),
    ];
    let (result, churn) = measure(|| cached_song_result(cached));
    assert_eq!(churn.allocs, 0);
    assert_eq!(churn.reallocs, 0);
    let p = result.plot;
    assert_eq!(
        [
            p.freq_domain.as_ptr(),
            p.beat_digest.as_ptr(),
            p.post_kernel.as_ptr(),
            p.times_ms.as_ptr(),
            p.convolution.as_ptr()
        ],
        pointers
    );
    assert_eq!(p.cols, 4096);
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_owned_pipelines_cached() {
    for (label, size, applied) in [
        ("empty", 0, false),
        ("small", 256, false),
        ("normal", 16384, false),
        ("large", 262144, false),
        ("applied", 256, true),
    ] {
        let cached = analysis(size, applied);
        compare(
            &format!("cached/{label}"),
            || {
                let input = black_box(cached.clone());
                black_box(before::cached_song_result(&input));
            },
            || {
                black_box(cached_song_result(black_box(cached.clone())));
            },
        );
    }
}
