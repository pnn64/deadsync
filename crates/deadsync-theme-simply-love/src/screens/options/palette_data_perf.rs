use super::*;
use crate::course_data_support::compare;
use crate::perf::measure;
use std::hint::black_box;

fn draw(
    out: &mut Vec<Actor>,
    state: &State,
    selected: usize,
    focus: Option<usize>,
    editing: bool,
    blink: f32,
    original: bool,
) {
    let id = &state.judgment_palettes.palettes[0].id;
    if original {
        course_data_original::push_editor(
            out,
            state,
            id,
            selected,
            focus,
            editing,
            "Palette Å猫",
            blink,
            Some("status"),
            [0.8, 0.2, 0.1, 1.],
            480.,
            270.,
            "wendy",
        );
    } else {
        push_editor(
            out,
            state,
            id,
            selected,
            focus,
            editing,
            "Palette Å猫",
            blink,
            Some("status"),
            [0.8, 0.2, 0.1, 1.],
            480.,
            270.,
            "wendy",
        );
    }
}

#[test]
fn palette_editor_preserves_channels_focus_cursor_and_actor_output() {
    let mut state = super::super::tests::init();
    for channel in [
        0.,
        1. / 255.,
        9. / 255.,
        10. / 255.,
        99. / 255.,
        100. / 255.,
        254. / 255.,
        1.,
        -1.,
        2.,
        f32::NAN,
        f32::INFINITY,
    ] {
        for (i, color) in state.judgment_palettes.palettes[0]
            .palette
            .colors
            .iter_mut()
            .enumerate()
        {
            *color = [channel, (i as f32) / 7., 1. - channel, 1.];
        }
        for selected in 0..=EDITOR_DONE_ROW + 1 {
            for focus in [None, Some(0), Some(1), Some(2), Some(3), Some(usize::MAX)] {
                for (editing, blink) in [(false, 0.), (true, 0.1), (true, 0.6)] {
                    let mut old = Vec::new();
                    let mut new = Vec::new();
                    draw(&mut old, &state, selected, focus, editing, blink, true);
                    draw(&mut new, &state, selected, focus, editing, blink, false);
                    assert_eq!(format!("{old:?}"), format!("{new:?}"));
                }
            }
        }
    }
}

#[test]
fn palette_editor_avoids_seven_intermediate_channel_vectors() {
    let state = super::super::tests::init();
    let mut old = Vec::with_capacity(64);
    let mut new = Vec::with_capacity(64);
    // Warm the translation service before recording allocations.
    draw(&mut old, &state, 1, Some(0), false, 0., true);
    old.clear();
    let (_, before) = measure(|| draw(&mut old, &state, 1, Some(0), false, 0., true));
    let (_, after) = measure(|| draw(&mut new, &state, 1, Some(0), false, 0., false));
    assert_eq!(before.allocs - after.allocs, 28);
    assert!(after.allocated_bytes < before.allocated_bytes);
    assert_eq!(format!("{old:?}"), format!("{new:?}"));
}

#[test]
#[ignore = "paired release benchmark; run with --ignored --nocapture --test-threads=1"]
fn benchmark_course_data_palette() {
    let state = super::super::tests::init();
    for (label, selected, focus, editing) in [
        ("idle", 0, None, false),
        ("adjust", 1, Some(1), false),
        ("rename", 0, None, true),
    ] {
        let mut old = Vec::with_capacity(64);
        let mut new = Vec::with_capacity(64);
        compare(
            &format!("palette/{label}"),
            || {
                old.clear();
                draw(
                    &mut old,
                    black_box(&state),
                    selected,
                    focus,
                    editing,
                    0.,
                    true,
                );
                black_box(&old);
            },
            || {
                new.clear();
                draw(
                    &mut new,
                    black_box(&state),
                    selected,
                    focus,
                    editing,
                    0.,
                    false,
                );
                black_box(&new);
            },
        );
    }
}
