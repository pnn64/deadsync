use super::*;
use crate::course_data_support::compare;
use crate::perf::measure;
use deadlib_present::actors::TextContent;
use std::hint::black_box;

fn ready_state() -> State {
    let mut state = super::super::state::init();
    std::sync::Arc::make_mut(&mut state.details).phase = DetailsPhase::Ready;
    let describe = std::sync::Arc::make_mut(&mut state.describe);
    describe.stamina = deadsync_online::smo_describe::ViewPhase::Ready;
    describe.all_around = deadsync_online::smo_describe::ViewPhase::Ready;
    state
}

fn draw(out: &mut Vec<Actor>, state: &State, original: bool) {
    if original {
        course_data_original::push_tabs(out, state);
        course_data_original::push_context_band(out, state, 960.0);
        course_data_original::push_footer(out, state, 960.0);
    } else {
        push_tabs(out, state);
        push_context_band(out, state, 960.0);
        push_footer(out, state, 960.0);
    }
}

fn normalized(mut actors: Vec<Actor>) -> String {
    for actor in &mut actors {
        if let Actor::Text { content, .. } = actor {
            *content = TextContent::Owned(content.as_str().to_owned());
        }
    }
    format!("{actors:?}")
}

#[test]
fn browser_chrome_preserves_all_tabs_zones_and_search_text() {
    let mut state = ready_state();
    for index in 0..TABS.len() {
        state.tab_index = index;
        for zone in [
            Zone::Tabs,
            Zone::Featured,
            Zone::Years,
            Zone::DoublesPick,
            Zone::DoublesRows,
            Zone::List,
            Zone::Installed,
            Zone::Detail,
        ] {
            state.zone = zone;
            for query in ["", "find \"Pack\"", "Å猫"] {
                state.query = query.into();
                assert!(!services_busy(&state));
                assert!(!band_busy(&state));
                let mut old = Vec::new();
                let mut new = Vec::new();
                draw(&mut old, &state, true);
                draw(&mut new, &state, false);
                assert_eq!(
                    normalized(old),
                    normalized(new),
                    "tab {index} zone {zone:?} query {query}"
                );
            }
        }
    }
}

#[test]
fn browser_chrome_does_not_copy_static_labels() {
    let state = ready_state();
    let mut old = Vec::with_capacity(128);
    let mut new = Vec::with_capacity(128);
    let (_, before) = measure(|| draw(&mut old, &state, true));
    let (_, after) = measure(|| draw(&mut new, &state, false));
    assert!(before.allocs - after.allocs >= TABS.len() + 1);
    assert!(after.allocated_bytes < before.allocated_bytes);
    assert_eq!(normalized(old), normalized(new));
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_course_data_chrome() {
    for (label, tab_index, query) in [
        ("pad", 1, ""),
        ("search", 0, "find pack"),
        ("beginner", 3, ""),
    ] {
        let mut state = ready_state();
        state.tab_index = tab_index;
        state.query = query.into();
        let mut old = Vec::with_capacity(128);
        let mut new = Vec::with_capacity(128);
        compare(
            &format!("chrome/{label}"),
            || {
                old.clear();
                draw(&mut old, black_box(&state), true);
                black_box(&old);
            },
            || {
                new.clear();
                draw(&mut new, black_box(&state), false);
                black_box(&new);
            },
        );
    }
}

#[test]
fn comma_formatting_preserves_decimal_boundaries_and_large_values() {
    for (n, expected) in [
        (0, "0"),
        (999, "999"),
        (1000, "1,000"),
        (1_234_567, "1,234,567"),
    ] {
        assert_eq!(commify(n), expected);
    }
    for n in 0..20_000 {
        assert_eq!(commify(n), course_data_original::commify(n));
    }
    let mut value = 1usize;
    loop {
        for n in [value - 1, value, value.saturating_add(1)] {
            assert_eq!(commify(n), course_data_original::commify(n));
        }
        let Some(next) = value.checked_mul(10) else {
            break;
        };
        value = next;
    }
    let mut random = 0x1234_5678_9abc_def0u64;
    for _ in 0..10_000 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let n = random as usize;
        assert_eq!(commify(n), course_data_original::commify(n));
    }
    assert_eq!(
        commify(usize::MAX),
        course_data_original::commify(usize::MAX)
    );
}

#[test]
fn comma_formatting_allocates_only_the_final_text() {
    for n in [0, 9, 999, 1_000, 123_456, 123_456_789, usize::MAX] {
        let (old, before) = measure(|| course_data_original::commify(n));
        let (new, after) = measure(|| commify(n));
        assert_eq!(old, new);
        assert_eq!(after.allocs, 1);
        assert_eq!(after.reallocs, 0);
        assert!(after.allocs < before.allocs);
        assert!(after.allocated_bytes < before.allocated_bytes);
    }
}

#[test]
fn readouts_preserve_counts_quoting_and_all_actor_properties() {
    let mut state = ready_state();
    for index in 0..TABS.len() {
        state.tab_index = index;
        for count in [0, 1, 7, 999, 1_000, 12_345] {
            state.results.resize(count, 0);
            for query in ["", "find \"Pack\"", "Å猫"] {
                state.query = query.into();
                let mut old = Vec::new();
                let mut new = Vec::new();
                course_data_original::push_readout(&mut old, &state, 960.0);
                push_readout(&mut new, &state, 960.0);
                assert_eq!(normalized(old), normalized(new));
            }
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_course_data_numbers() {
    for n in [0, 9, 999, 1_000, 123_456, 123_456_789, usize::MAX] {
        compare(
            &format!("commify/{n}"),
            || {
                black_box(course_data_original::commify(black_box(n)));
            },
            || {
                black_box(commify(black_box(n)));
            },
        );
    }
    for (label, index, query) in [("pad", 1, ""), ("years", 7, ""), ("search", 0, "find pack")] {
        let mut state = ready_state();
        state.tab_index = index;
        state.query = query.into();
        state.results.resize(12_345, 0);
        compare(
            &format!("readout/{label}"),
            || {
                black_box(course_data_original::readout_text(black_box(&state)));
            },
            || {
                black_box(readout_text(black_box(&state)));
            },
        );
    }
}
