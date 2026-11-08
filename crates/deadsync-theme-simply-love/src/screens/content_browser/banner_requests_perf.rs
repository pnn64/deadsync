use super::super::owned_results_support as support;
use super::banner_requests_original as original;
use super::*;
use crate::perf::measure;
use deadsync_online::pack_page::PagePhase;
use std::hint::black_box;

#[test]
fn borrowed_banner_windows_preserve_order_duplicates_and_missing_indices() {
    for count in [0, 1, 7, 24, 200] {
        let mut state = support::fixture(count, true);
        for position in [0, 1, count / 2, count, count + 10] {
            state.cursor = position;
            state.doubles_window = [position, position / 2];
            assert_eq!(wanted_banners(&state), original::wanted_banners(&state));
        }
        state.featured.extend([0, count + 100, 0]);
        state.results.extend([count + 100, 0, 0]);
        state.doubles_left.extend([0, count + 100, 0]);
        state.doubles_right.extend([count + 100, 0, 0]);
        state.cursor = 0;
        state.doubles_window = [0, 0];
        let indices = (
            &state.featured,
            &state.results,
            &state.doubles_left,
            &state.doubles_right,
        );
        let before = format!("{indices:?}");
        let current = wanted_banners(&state);
        assert_eq!(current, original::wanted_banners(&state));
        assert_eq!(before, format!("{indices:?}"));
        if count != 0 {
            assert!(current.iter().filter(|(id, _)| *id == 1_000_000).count() > 1);
        }
    }
}

#[test]
fn borrowed_banner_requests_keep_source_precedence_and_detail_jackets() {
    let mut state = support::fixture(24, true);
    state
        .popular_banner
        .insert(1_000_000, "ranked small".into());
    state
        .popular_art
        .insert(1_000_001, "ranked fallback".into());
    support::page(&mut state, PagePhase::Ready, false);
    for zone in [Zone::List, Zone::DoublesRows, Zone::Detail] {
        state.zone = zone;
        for window in [0, 1, 7, 19, 20, 21] {
            state.song_window = window;
            let current = wanted_banners(&state);
            assert_eq!(current, original::wanted_banners(&state));
            assert_eq!(current[0], (1_000_000, "ranked small".into()));
            assert!(
                current
                    .iter()
                    .any(|(id, url)| *id == 1_000_001 && url.ends_with("1000001.jpg"))
            );
            assert_eq!(
                current.iter().any(|(id, _)| id & (1 << 63) != 0),
                zone == Zone::Detail && window < 20
            );
        }
    }
}

#[test]
fn banner_requests_remove_all_four_temporary_index_allocations() {
    for banners in [false, true] {
        let state = support::fixture(200, banners);
        let (before, old) = measure(|| original::wanted_banners(&state));
        let (after, new) = measure(|| wanted_banners(&state));
        assert_eq!(after, before);
        assert_eq!(old.allocs - new.allocs, 4);
        assert!(new.allocated_bytes < old.allocated_bytes);
        assert_eq!(new.reallocs, old.reallocs);
        if !banners {
            assert_eq!(new, crate::perf::Churn::default());
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_banner_requests() {
    for (count, banners, detail) in [
        (0, false, false),
        (1, true, false),
        (24, false, false),
        (24, true, false),
        (200, true, false),
        (9500, true, false),
        (200, true, true),
    ] {
        let mut state = support::fixture(count, banners);
        if detail {
            support::page(&mut state, PagePhase::Ready, false);
            state.zone = Zone::Detail;
            state.song_window = 7;
        }
        let label = format!(
            "banners-{count}-{}-{}",
            if banners { "urls" } else { "missing" },
            if detail { "detail" } else { "list" }
        );
        let (old, before) = measure(|| original::wanted_banners(&state));
        let (new, after) = measure(|| wanted_banners(&state));
        assert_eq!(new, old);
        println!("{label} churn: original {before:?}, current {after:?}");
        support::paired_bench::compare(&label, 100, |current| {
            black_box(if current {
                wanted_banners(black_box(&state))
            } else {
                original::wanted_banners(black_box(&state))
            });
        });
    }
}
