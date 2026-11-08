use super::super::owned_results_support as support;
use super::detail_facts_original as original;
use super::*;
use crate::perf::measure;
use deadlib_present::actors::TextContent;
use std::hint::black_box;
use std::sync::Arc;

fn draw(state: &State, width: f32, current: bool) -> Vec<Actor> {
    let mut actors = Vec::with_capacity(32);
    let pack = &state.snapshot.catalog[0];
    if current {
        push_fact_table(&mut actors, state, pack, width);
    } else {
        original::push_fact_table(&mut actors, state, pack, width);
    }
    actors
}

#[test]
fn moved_detail_values_preserve_all_labels_layout_and_loading_spinners() {
    for phase in [
        PagePhase::Idle,
        PagePhase::Loading,
        PagePhase::Ready,
        PagePhase::Error,
    ] {
        for long in [false, true] {
            let mut state = support::fixture(1, true);
            support::page(&mut state, phase, long);
            for width in [100.0, 300.0, 600.0] {
                let (_, after) = support::compare_draws(|current| draw(&state, width, current));
                let labels = after
                    .iter()
                    .filter_map(|actor| match actor {
                        Actor::Text {
                            content: TextContent::Static(label),
                            ..
                        } => Some(*label),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    labels,
                    facts(&state, &state.snapshot.catalog[0])
                        .iter()
                        .map(|(label, _)| *label)
                        .collect::<Vec<_>>()
                );
            }
            Arc::make_mut(&mut state.page).pack_id = 99;
            support::compare_draws(|current| draw(&state, 300.0, current));
        }
    }
}

#[test]
fn detail_facts_preserve_partial_metadata_and_unusual_catalog_values() {
    let mut state = support::fixture(1, false);
    support::page(&mut state, PagePhase::Ready, false);
    let pack = &mut Arc::make_mut(&mut state.snapshot).catalog;
    let pack = &mut Arc::make_mut(pack)[0];
    pack.song_count = u32::MAX;
    pack.size_bytes = u64::MAX;
    pack.pack_type = Some("KeYbOaRd".into());
    for page in [
        PackPage::default(),
        PackPage {
            chart_count: Some("".into()),
            meter_labels: vec![9],
            authors: vec!["\u{6771}\u{4eac}\nA".into()],
            ..PackPage::default()
        },
    ] {
        Arc::make_mut(&mut state.page).page = Some(Arc::new(page));
        support::compare_draws(|current| draw(&state, 300.0, current));
    }
}

#[test]
fn detail_facts_remove_label_and_value_copy_allocations() {
    for phase in [PagePhase::Loading, PagePhase::Ready, PagePhase::Error] {
        for long in [false, true] {
            let mut state = support::fixture(1, true);
            support::page(&mut state, phase, long);
            support::compare_draws(|current| draw(&state, 300.0, current));
            let mut old = Vec::with_capacity(32);
            let mut new = Vec::with_capacity(32);
            let pack = &state.snapshot.catalog[0];
            let (_, before) = measure(|| original::push_fact_table(&mut old, &state, pack, 300.0));
            let (_, after) = measure(|| push_fact_table(&mut new, &state, pack, 300.0));
            let nonempty = facts(&state, pack)
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .count();
            assert_eq!(before.allocs - after.allocs, 8 + nonempty);
            assert_eq!(before.reallocs, after.reallocs);
            assert!(after.allocated_bytes < before.allocated_bytes);
            assert!(support::retained_text_bytes(&new) <= support::retained_text_bytes(&old));
        }
    }
}

#[test]
#[ignore = "paired release benchmark; run explicitly with --ignored --nocapture"]
fn benchmark_detail_fact_table() {
    for (name, phase, long, banners) in [
        ("idle", PagePhase::Idle, false, false),
        ("loading", PagePhase::Loading, false, false),
        ("ready", PagePhase::Ready, false, true),
        ("long-authors", PagePhase::Ready, true, true),
        ("error", PagePhase::Error, false, false),
    ] {
        let mut state = support::fixture(1, banners);
        support::page(&mut state, phase, long);
        support::compare_draws(|current| draw(&state, 300.0, current));
        let label = format!("facts-{name}");
        let mut old = Vec::with_capacity(32);
        let mut new = Vec::with_capacity(32);
        let pack = &state.snapshot.catalog[0];
        let (_, before) = measure(|| original::push_fact_table(&mut old, &state, pack, 300.0));
        let (_, after) = measure(|| push_fact_table(&mut new, &state, pack, 300.0));
        println!("{label} churn: original {before:?}, current {after:?}");
        println!(
            "{label} retained text bytes: original {}, current {}",
            support::retained_text_bytes(&old),
            support::retained_text_bytes(&new)
        );
        support::paired_bench::compare_prepared(
            &label,
            50,
            || Vec::with_capacity(32),
            |mut actors, current| {
                if current {
                    push_fact_table(&mut actors, black_box(&state), black_box(pack), 300.0);
                } else {
                    original::push_fact_table(
                        &mut actors,
                        black_box(&state),
                        black_box(pack),
                        300.0,
                    );
                }
                black_box(actors);
            },
        );
    }
}
