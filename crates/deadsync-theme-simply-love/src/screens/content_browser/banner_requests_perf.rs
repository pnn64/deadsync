use super::super::owned_results_support as support;

use super::*;
use deadsync_online::pack_page::PagePhase;

#[test]
fn borrowed_banner_windows_preserve_order_duplicates_and_missing_indices() {
    for count in [0, 1, 7, 24, 200] {
        let mut state = support::fixture(count, true);
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
