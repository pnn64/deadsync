use super::*;
use crate::buffers_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn original_count(count: u32) -> Arc<str> {
    buffers_original::format_total_songs_played(count).into()
}

#[test]
fn profile_count_preserves_localized_singular_plural_and_large_counts() {
    for count in (0..=1000).chain([9_999, 10_000, 999_999, u32::MAX]) {
        assert_eq!(format_total_songs_played(count), original_count(count));
    }
    assert_ne!(format_total_songs_played(1), format_total_songs_played(2));
}

#[test]
fn profile_count_removes_the_shared_owned_shared_round_trip() {
    for count in [0, 1, 999, u32::MAX] {
        black_box(original_count(count)); // Initialize localization outside tracking.
        let (old, before) = measure(|| original_count(count));
        let (new, after) = measure(|| format_total_songs_played(count));
        assert_eq!(old, new);
        assert_eq!(before.allocs - after.allocs, 2);
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_transient_buffers_profile_counts() {
    for count in [0, 1, 999, u32::MAX] {
        compare(
            &format!("profile/{count}"),
            || {
                black_box(original_count(black_box(count)));
            },
            || {
                black_box(format_total_songs_played(black_box(count)));
            },
        );
    }
}
