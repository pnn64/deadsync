use super::*;
use deadsync_theme::views::GraphicsVideoModeView;
use std::hint::black_box;

mod original {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/perf/option_original.rs"
    ));
}

fn set_graphics(state: &mut State, id: SubRowId, value: usize) {
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].choice_indices,
        GRAPHICS_OPTIONS_ROWS,
        id,
        value,
    );
    set_choice_by_id(
        &mut state.sub[SubmenuKind::Graphics].cursor_indices,
        GRAPHICS_OPTIONS_ROWS,
        id,
        value,
    );
}

fn modes(count: usize) -> Vec<GraphicsVideoModeView> {
    let sizes = [
        (1920, 1080),
        (1024, 768),
        (1280, 720),
        (3840, 780),
        (800, 600),
        (0, 0),
        (0, 720),
        (1920, 1200),
        (608, 608),
    ];
    (0..count)
        .map(|i| {
            let (width, height) = sizes[(i * 7) % sizes.len()];
            GraphicsVideoModeView {
                width,
                height,
                refresh_rate_millihertz: [60_000, 59_940, 144_000, 0, 120_000, 240_000][i % 6],
            }
        })
        .collect()
}

fn graphics_state(count: usize, windowed: bool) -> State {
    let mut state = init();
    state.monitor_specs = vec![GraphicsMonitorView {
        name: "Bench monitor".into(),
        modes: modes(count),
    }];
    state.display_mode_choices = vec!["Monitor".into(), "Windowed".into()];
    state.resolution_choices = vec![(1920, 1080)];
    state.refresh_rate_choices = vec![0, 59_940];
    state.refresh_rate_at_load = 59_940;
    state.max_fps_at_load = 0;
    set_graphics(&mut state, SubRowId::DisplayMode, usize::from(windowed));
    set_graphics(&mut state, SubRowId::DisplayResolution, 0);
    set_graphics(&mut state, SubRowId::DisplayAspectRatio, 0);
    set_graphics(&mut state, SubRowId::RefreshRate, 1);
    set_graphics(&mut state, SubRowId::MaxFps, 0);
    state
}

fn assert_graphics_equal(a: &State, b: &State) {
    assert_eq!(a.resolution_choices, b.resolution_choices);
    assert_eq!(a.refresh_rate_choices, b.refresh_rate_choices);
    for kind in SubmenuKind::ALL {
        assert_eq!(
            a.sub[kind].choice_indices, b.sub[kind].choice_indices,
            "{kind:?} choices"
        );
        assert_eq!(
            a.sub[kind].cursor_indices, b.sub[kind].cursor_indices,
            "{kind:?} cursors"
        );
    }
}

fn sound_state(rates: &[u32]) -> State {
    let mut state = init();
    state.sound_device_options = vec![SoundDeviceOption {
        label: "Auto".into(),
        config_index: None,
        sample_rates_hz: rates.to_vec(),
    }];
    set_sound_choice_index(&mut state, SubRowId::SoundDevice, 0);
    state
}

#[test]
fn sample_rates_preserve_order_fallbacks_duplicates_and_bounds() {
    let mut state = sound_state(&[]);
    let mut seed = 7u32;
    for len in 0..130 {
        let rates: Vec<u32> = (0..len)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                [0, 44_100, 48_000, 96_000, 192_000, u32::MAX][seed as usize % 6]
            })
            .collect();
        state.sound_device_options[0].sample_rates_hz = rates;
        for device in [0, 1, usize::MAX] {
            set_sound_choice_index(&mut state, SubRowId::SoundDevice, device);
            let expected = original::sound_sample_rate_choices(&state);
            assert_eq!(sound_sample_rate_choices(&state), expected);
            for index in (0..expected.len() + 3).chain([usize::MAX]) {
                assert_eq!(
                    sample_rate_from_choice(&state, index),
                    original::sample_rate_from_choice(&state, index)
                );
            }
            for rate in [
                None,
                Some(0),
                Some(1),
                Some(44_100),
                Some(192_000),
                Some(u32::MAX),
            ] {
                assert_eq!(
                    sample_rate_choice_index(&state, rate),
                    original::sample_rate_choice_index(&state, rate)
                );
            }
        }
    }
    state.sound_device_options.clear();
    assert_eq!(
        sound_sample_rate_choices(&state),
        [None, Some(44_100), Some(48_000)]
    );
}

#[test]
fn sample_rate_labels_match_owned_and_shared_menu_choices() {
    for rates in [
        vec![],
        vec![0],
        vec![44_100, 48_000, 96_000, 192_000],
        vec![48_000, 48_000, 0, 44_100],
    ] {
        let state = sound_state(&rates);
        let row = sound_row_index(SubRowId::AudioSampleRate).unwrap();
        assert_eq!(
            original::collect_row_choices::<Cow<'static, str>>(
                &state,
                SubmenuKind::Sound,
                SOUND_OPTIONS_ROWS,
                row
            ),
            row_choices(&state, SubmenuKind::Sound, SOUND_OPTIONS_ROWS, row)
        );
        assert_eq!(
            original::collect_row_choices::<Arc<str>>(
                &state,
                SubmenuKind::Sound,
                SOUND_OPTIONS_ROWS,
                row
            ),
            submenu_display_choice_texts(&state, SubmenuKind::Sound, SOUND_OPTIONS_ROWS, row)
        );
    }
}

#[test]
fn sample_rate_queries_have_no_allocation_churn() {
    for rates in [&[][..], &[44_100, 48_000, 44_100, 0, 96_000][..]] {
        let state = sound_state(rates);
        crate::perf::assert_no_churn(|| {
            black_box(sample_rate_from_choice(black_box(&state), 2));
            black_box(sample_rate_choice_index(black_box(&state), Some(48_000)));
        });
    }
}

#[test]
fn resolution_rebuild_matches_original_across_modes_and_transitions() {
    for count in [0, 1, 16, 128] {
        let mut before = graphics_state(count, false);
        let mut after = graphics_state(count, false);
        for aspect in [0, 1, 2, 3, 99] {
            for selected in [(0, 0), (1920, 1080), (3840, 780), (1, 0), (777, 777)] {
                set_graphics(&mut before, SubRowId::DisplayAspectRatio, aspect);
                set_graphics(&mut after, SubRowId::DisplayAspectRatio, aspect);
                original::rebuild_resolution_choices(&mut before, selected.0, selected.1);
                rebuild_resolution_choices(&mut after, selected.0, selected.1);
                assert_graphics_equal(&before, &after);
            }
        }
    }
}

#[test]
fn refresh_rebuild_preserves_zero_duplicates_ties_thresholds_and_selection() {
    for windowed in [false, true] {
        let mut before = graphics_state(0, windowed);
        let mut after = graphics_state(0, windowed);
        let data: Vec<_> = [70_000, 50_000, 0, 60_000, 50_000, 144_000]
            .into_iter()
            .map(|rate| GraphicsVideoModeView {
                width: 1920,
                height: 1080,
                refresh_rate_millihertz: rate,
            })
            .collect();
        before.monitor_specs[0].modes = data.clone();
        after.monitor_specs[0].modes = data;
        for rate in [
            0, 1, 49_999, 55_000, 59_940, 60_000, 80_000, 134_000, 134_001, 154_000,
        ] {
            for index in [0, 1, 99] {
                before.refresh_rate_choices = vec![0, rate];
                after.refresh_rate_choices = vec![0, rate];
                set_graphics(&mut before, SubRowId::RefreshRate, index);
                set_graphics(&mut after, SubRowId::RefreshRate, index);
                original::rebuild_refresh_rate_choices(&mut before);
                rebuild_refresh_rate_choices(&mut after);
                assert_graphics_equal(&before, &after);
                if !windowed {
                    assert_eq!(&after.refresh_rate_choices[..2], &[0, 0]);
                }
            }
        }
        for display in [99, 0, 1, 0] {
            set_graphics(&mut before, SubRowId::DisplayMode, display);
            set_graphics(&mut after, SubRowId::DisplayMode, display);
            original::rebuild_refresh_rate_choices(&mut before);
            rebuild_refresh_rate_choices(&mut after);
            assert_graphics_equal(&before, &after);
        }
    }
}

#[test]
fn max_fps_seed_matches_original_without_materializing_rates() {
    for count in [0, 1, 16, 256] {
        for windowed in [false, true] {
            let mut state = graphics_state(count, windowed);
            state.refresh_rate_choices = vec![0];
            set_graphics(&mut state, SubRowId::RefreshRate, 0);
            for size in [(1920, 1080), (123, 456)] {
                state.resolution_choices[0] = size;
                for rate in [0, 1, 60, 240, u16::MAX] {
                    assert_eq!(
                        max_fps_seed_value(&state, rate),
                        original::max_fps_seed_value(&state, rate)
                    );
                }
            }
        }
    }
}

#[test]
fn warmed_graphics_rebuilds_reuse_storage_without_churn() {
    let mut state = graphics_state(256, false);
    rebuild_resolution_choices(&mut state, 1920, 1080);
    let pointers = (
        state.resolution_choices.as_ptr(),
        state.refresh_rate_choices.as_ptr(),
    );
    crate::perf::assert_no_churn(|| {
        for _ in 0..10 {
            rebuild_resolution_choices(black_box(&mut state), 1920, 1080);
            rebuild_refresh_rate_choices(black_box(&mut state));
        }
    });
    assert_eq!(
        pointers,
        (
            state.resolution_choices.as_ptr(),
            state.refresh_rate_choices.as_ptr()
        )
    );
    set_graphics(&mut state, SubRowId::DisplayMode, 1);
    crate::perf::assert_no_churn(|| rebuild_refresh_rate_choices(black_box(&mut state)));
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_option_enumeration_sound() {
    for (name, rates) in [
        ("fallback", vec![]),
        ("typical", vec![44_100, 48_000, 96_000, 192_000]),
        (
            "duplicates",
            (0..128)
                .map(|i| [48_000, 44_100, 48_000, 96_000][i % 4])
                .collect(),
        ),
        ("unique", (0..64).map(|i| 8000 + i * 1000).collect()),
    ] {
        let state = sound_state(&rates);
        crate::option_perf::compare(
            &format!("sound/{name}/scan"),
            || {
                black_box(
                    original::sound_sample_rate_choices(black_box(&state))
                        .into_iter()
                        .fold(0u64, |n, h| n + u64::from(h.unwrap_or(0))),
                );
            },
            || {
                black_box(
                    sound_sample_rate_choices(black_box(&state))
                        .into_iter()
                        .fold(0u64, |n, h| n + u64::from(h.unwrap_or(0))),
                );
            },
        );
        crate::option_perf::compare(
            &format!("sound/{name}/query"),
            || {
                black_box(original::sample_rate_from_choice(black_box(&state), 2));
                black_box(original::sample_rate_choice_index(
                    black_box(&state),
                    Some(48_000),
                ));
            },
            || {
                black_box(sample_rate_from_choice(black_box(&state), 2));
                black_box(sample_rate_choice_index(black_box(&state), Some(48_000)));
            },
        );
        let row = sound_row_index(SubRowId::AudioSampleRate).unwrap();
        crate::option_perf::compare(
            &format!("sound/{name}/labels"),
            || {
                black_box(original::collect_row_choices::<Cow<'static, str>>(
                    black_box(&state),
                    SubmenuKind::Sound,
                    SOUND_OPTIONS_ROWS,
                    row,
                ));
            },
            || {
                black_box(row_choices(
                    black_box(&state),
                    SubmenuKind::Sound,
                    SOUND_OPTIONS_ROWS,
                    row,
                ));
            },
        );
        assert_eq!(
            original::collect_row_choices::<Cow<'static, str>>(
                &state,
                SubmenuKind::Sound,
                SOUND_OPTIONS_ROWS,
                row
            ),
            row_choices(&state, SubmenuKind::Sound, SOUND_OPTIONS_ROWS, row)
        );
    }
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_option_enumeration_graphics() {
    for (name, count, windowed) in [
        ("fallback", 0, false),
        ("typical", 64, false),
        ("many_modes", 1024, false),
        ("windowed", 64, true),
    ] {
        let mut before = graphics_state(count, windowed);
        let mut after = graphics_state(count, windowed);
        crate::option_perf::compare(
            &format!("resolution/{name}"),
            || {
                original::resolution_with_current_refresh(black_box(&mut before), 1920, 1080);
                black_box(&before.resolution_choices);
            },
            || {
                rebuild_resolution_choices(black_box(&mut after), 1920, 1080);
                black_box(&after.resolution_choices);
            },
        );
        assert_graphics_equal(&before, &after);
        crate::option_perf::compare(
            &format!("refresh/{name}"),
            || {
                original::rebuild_refresh_rate_choices(black_box(&mut before));
                black_box(&before.refresh_rate_choices);
            },
            || {
                rebuild_refresh_rate_choices(black_box(&mut after));
                black_box(&after.refresh_rate_choices);
            },
        );
        assert_graphics_equal(&before, &after);
        before.refresh_rate_choices = vec![0];
        after.refresh_rate_choices = vec![0];
        set_graphics(&mut before, SubRowId::RefreshRate, 0);
        set_graphics(&mut after, SubRowId::RefreshRate, 0);
        crate::option_perf::compare(
            &format!("fps/{name}"),
            || {
                black_box(original::max_fps_seed_value(black_box(&before), 0));
            },
            || {
                black_box(max_fps_seed_value(black_box(&after), 0));
            },
        );
    }
}
