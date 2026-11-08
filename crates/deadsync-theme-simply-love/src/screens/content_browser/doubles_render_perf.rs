use super::super::owned_results_support as support;
use super::doubles_render_original as original;
use super::*;
use crate::perf::measure;
use std::hint::black_box;
use std::sync::Arc;

fn draw(state: &State, current: bool) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(256);
    if current {
        push_doubles(&mut actors, state);
    } else {
        original::push_doubles(&mut actors, state);
    }
    actors
}

#[test]
fn borrowed_doubles_columns_preserve_all_drawn_fields_and_scrollbars() {
    for count in [0, 1, 2, 7, 24, 200, 9500] {
        let mut state = support::fixture(count, true);
        for position in [0, count / 4, count, count + 10] {
            state.doubles_window = [position, position / 2];
            for column in 0..2 {
                state.doubles_column = column;
                for zone in [Zone::List, Zone::DoublesRows] {
                    state.zone = zone;
                    support::compare_draws(|current| draw(&state, current));
                }
            }
        }
        state.doubles_window = [0, 0];
        state.doubles_left = vec![count + 1, 0, 0, count + 10];
        state.doubles_right = vec![0, count + 1, 0];
        let before = (state.doubles_left.clone(), state.doubles_right.clone());
        support::compare_draws(|current| draw(&state, current));
        assert_eq!(before, (state.doubles_left, state.doubles_right));
    }
}

#[test]
fn borrowed_doubles_columns_keep_loading_empty_and_failed_artwork_states() {
    for count in [0, 3, 24] {
        let mut state = support::fixture(count, true);
        for phase in [
            DetailsPhase::Idle,
            DetailsPhase::Loading,
            DetailsPhase::Ready,
            DetailsPhase::Error,
        ] {
            Arc::make_mut(&mut state.details).phase = phase;
            for waiting in [false, true] {
                state.doubles_left_waiting = waiting;
                Arc::make_mut(&mut state.itgdb).phase = if waiting {
                    ItgdbPhase::Loading
                } else {
                    ItgdbPhase::Error
                };
                support::compare_draws(|current| draw(&state, current));
                Arc::make_mut(&mut state.banner_failed).extend([1_000_000, 1_000_001]);
                support::compare_draws(|current| draw(&state, current));
            }
        }
    }
}

#[test]
fn doubles_rendering_no_longer_allocates_or_copies_full_column_lists() {
    for count in [0, 1, 24, 200, 9500] {
        let state = support::fixture(count, false);
        support::compare_draws(|current| draw(&state, current));
        let mut old = Vec::with_capacity(256);
        let mut new = Vec::with_capacity(256);
        let (_, before) = measure(|| original::push_doubles(&mut old, &state));
        let (_, after) = measure(|| push_doubles(&mut new, &state));
        let columns = usize::from(!state.doubles_left.is_empty())
            + usize::from(!state.doubles_right.is_empty());
        assert_eq!(before.allocs - after.allocs, columns);
        assert_eq!(
            before.allocated_bytes - after.allocated_bytes,
            count * std::mem::size_of::<usize>()
        );
        assert_eq!(before.reallocs, after.reallocs);
        assert_eq!(
            support::retained_text_bytes(&old),
            support::retained_text_bytes(&new)
        );
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_doubles_rendering() {
    for (count, scrolled, banners) in [
        (0, false, false),
        (1, false, false),
        (24, false, false),
        (200, false, false),
        (9500, false, false),
        (9500, true, false),
        (9500, true, true),
    ] {
        let mut state = support::fixture(count, banners);
        if scrolled {
            state.doubles_window = [count / 4, count / 5];
        }
        support::compare_draws(|current| draw(&state, current));
        let label = format!(
            "doubles-{count}-{}-{}",
            if scrolled { "scrolled" } else { "top" },
            if banners { "waiting-art" } else { "no-art" }
        );
        let mut old = Vec::with_capacity(256);
        let mut new = Vec::with_capacity(256);
        let (_, before) = measure(|| original::push_doubles(&mut old, &state));
        let (_, after) = measure(|| push_doubles(&mut new, &state));
        println!("{label} churn: original {before:?}, current {after:?}");
        println!(
            "{label} retained text bytes: original {}, current {}",
            support::retained_text_bytes(&old),
            support::retained_text_bytes(&new)
        );
        support::paired_bench::compare_prepared(
            &label,
            10,
            || Vec::with_capacity(256),
            |mut actors, current| {
                if current {
                    push_doubles(&mut actors, black_box(&state));
                } else {
                    original::push_doubles(&mut actors, black_box(&state));
                }
                black_box(actors);
            },
        );
    }
}
