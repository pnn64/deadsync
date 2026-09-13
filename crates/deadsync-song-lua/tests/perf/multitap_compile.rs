use super::*;
use std::{cell::Cell, hint::black_box};

#[path = "multitap_compile_baseline.rs"]
mod baseline;

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}

fn random(seed: &mut u64) -> u32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (*seed >> 32) as u32
}

fn quantizable_in_debug(value: f32) -> f32 {
    // The parent quantizer uses i32 arithmetic. Exercise the complete f32
    // domain in release, where it wraps, and avoid its existing debug overflow.
    if cfg!(debug_assertions) && value.is_finite() {
        value.clamp(-1_000_000.0, 1_000_000.0)
    } else {
        value
    }
}

#[test]
fn multitap_compile_visibility_matches_geometry_including_unordered_floats() {
    let edges = [
        f32::NEG_INFINITY,
        quantizable_in_debug(-f32::MAX),
        -8.0,
        -f32::EPSILON,
        -0.0,
        0.0,
        f32::from_bits(1),
        f32::EPSILON,
        1.0,
        8.0,
        quantizable_in_debug(f32::MAX),
        f32::INFINITY,
        f32::NAN,
    ];
    for first in edges {
        for last in edges {
            for taps in [vec![first], vec![first, last], vec![first, 0.0, last]] {
                let desc = MultitapDesc {
                    lane: 1,
                    taps,
                    peak: None,
                };
                for beat in edges {
                    assert_eq!(
                        multitap_is_visible(&desc, beat),
                        calc_multitap_phase(&desc, beat).visible,
                        "taps={:?}, beat={beat:?}",
                        desc.taps
                    );
                }
            }
        }
    }
    let mut seed = 92;
    for count in [1, 2, 8, 32] {
        for _ in 0..256 {
            let mut taps: Vec<_> = (0..count)
                .map(|_| quantizable_in_debug(f32::from_bits(random(&mut seed))))
                .collect();
            for sorted in [false, true] {
                if sorted {
                    taps.sort_by(f32::total_cmp);
                }
                let desc = MultitapDesc {
                    lane: 1,
                    taps: taps.clone(),
                    peak: Some(f32::NAN),
                };
                for &tap in &taps {
                    for beat in [
                        tap,
                        tap.next_up(),
                        tap.next_down(),
                        tap - 8.0,
                        multitap_visible_start(tap),
                        f32::from_bits(random(&mut seed)),
                    ] {
                        assert_eq!(
                            multitap_is_visible(&desc, beat),
                            calc_multitap_phase(&desc, beat).visible,
                            "taps={taps:?}, beat={beat:?}"
                        );
                    }
                }
            }
        }
    }
    let desc = MultitapDesc {
        lane: 1,
        taps: vec![8.0, 10.0, 12.0],
        peak: None,
    };
    assert!(!multitap_is_visible(&desc, 0.0));
    assert!(multitap_is_visible(&desc, multitap_visible_start(8.0)));
    assert!(multitap_is_visible(&desc, 12.0));
    assert!(!multitap_is_visible(&desc, 12.0_f32.next_up()));
}

fn descriptions(count: usize, overlap: bool) -> Vec<MultitapDesc> {
    (0..count)
        .map(|i| {
            let start = i as f32 * if overlap { 0.125 } else { 16.0 };
            MultitapDesc {
                lane: 1 + i % 4,
                taps: (0..8).map(|tap| start + tap as f32 * 0.5).collect(),
                peak: if i % 2 == 0 { None } else { Some(1.5) },
            }
        })
        .collect()
}

#[test]
fn multitap_compile_explosions_preserve_boundaries_lane_filter_and_output_order() {
    let context = SongLuaCompileContext::new(".", "Visibility regression");
    for count in [0, 1, 8, 128] {
        for overlap in [false, true] {
            let mut descs = descriptions(count, overlap);
            descs.extend([
                MultitapDesc {
                    lane: 2,
                    taps: vec![-0.0, 0.0, f32::EPSILON],
                    peak: None,
                },
                MultitapDesc {
                    lane: 3,
                    taps: vec![8.0, 8.0, 10.0],
                    peak: Some(0.0),
                },
                MultitapDesc {
                    lane: 4,
                    taps: vec![16.0, 2.0, 32.0],
                    peak: Some(-1.0),
                },
                MultitapDesc {
                    lane: 8,
                    taps: vec![f32::MAX / 2.0],
                    peak: None,
                },
            ]);
            for lane in 1..=8 {
                let state = SongLuaOverlayState {
                    visible: false,
                    x: 23.0,
                    zoom: 0.8,
                    ..Default::default()
                };
                let prefix = vec![curve(0)];
                let mut old = prefix.clone();
                let mut new = prefix;
                baseline::push_multitap_explosion_eases(&mut old, 3, state, &context, &descs, lane);
                push_multitap_explosion_eases(&mut new, 3, state, &context, &descs, lane);
                assert_eq!(new, old, "count={count}, overlap={overlap}, lane={lane}");
            }
        }
    }
}

#[test]
fn multitap_compile_colors_preserve_case_unicode_and_match_precedence() {
    let keys = [
        "color",
        "rainbow",
        "solo",
        "horse",
        "toonprints",
        "cel",
        "cyber",
        "delta",
        "ddrlike",
        "enchantment",
        "excel",
        "metal",
        "onlyonecouples",
        "scalable",
        "spotlight",
        "vel",
        "vintage",
        "ascii",
        "default",
        "easy",
        "exact",
        "lambda",
        "note",
        "retro",
        "trax",
        "",
    ];
    for key in keys {
        for other in keys {
            for name in [
                format!("雪-{key}-{other}-é"),
                format!("{}-{}", key.to_uppercase(), other),
                format!("{key}-{}-{other}", "unrecognized".repeat(128)),
            ] {
                assert_eq!(
                    multitap_qtzn_color_table(&name),
                    baseline::multitap_qtzn_color_table(&name),
                    "{name}"
                );
            }
        }
    }
    assert_eq!(
        multitap_qtzn_color_table("RaInBoW-CoLoR"),
        &MULTITAP_QTZN_COLOR
    );
    assert_eq!(
        multitap_qtzn_color_table("NOTE-CyBeR"),
        &MULTITAP_QTZN_SHADOW
    );
    assert_eq!(multitap_qtzn_color_table("雪-Éİß"), &MULTITAP_QTZN_VIVID);
    for len in [15, 16, 17, 18, 64, 128] {
        for key in ["CoLoR", "NoTe", "unknw"] {
            let name = format!("{}{key}", "x".repeat(len - key.len()));
            assert_eq!(
                multitap_qtzn_color_table(&name),
                baseline::multitap_qtzn_color_table(&name)
            );
        }
    }
}

fn curve(index: usize) -> SongLuaOverlayEase {
    SongLuaOverlayEase {
        overlay_index: index + 17,
        unit: SongLuaTimeUnit::Beat,
        start: index as f32 + 1.0,
        limit: 0.5,
        span_mode: SongLuaSpanMode::Len,
        from: SongLuaOverlayStateDelta {
            y: Some(10.0),
            x: Some(2.0),
            zoom_y: Some(0.8),
            ..Default::default()
        },
        to: SongLuaOverlayStateDelta {
            y: Some(if index % 2 == 0 { 20.0 } else { 5.0 }),
            x: Some(4.0),
            zoom_y: Some(1.2),
            ..Default::default()
        },
        easing: Some("linear".into()),
        sustain: Some(0.25),
        opt1: Some(3.0),
        opt2: Some(4.0),
    }
}

#[test]
fn multitap_compile_curves_preserve_skips_metadata_components_and_order() {
    let mut input: Vec<_> = (0..16).map(curve).collect();
    input[1].limit = f32::EPSILON;
    input[2].limit = 0.0;
    input[3].start = 0.0;
    input[4].from.y = None;
    input[5].to.y = None;
    input[6].to.y = input[6].from.y;
    input[7].easing = None;
    input[8].span_mode = SongLuaSpanMode::End;
    input[9].unit = SongLuaTimeUnit::Second;
    for first in [0, 1, 8, input.len()] {
        let mut old = input.clone();
        let mut new = input.clone();
        baseline::split_multitap_y_eases(&mut old, first, 0.0);
        split_multitap_y_eases(&mut new, first, 0.0);
        assert_eq!(new, old);
        assert_eq!(&new[..first], &input[..first]);
    }
    let mut golden = vec![curve(0), curve(1)];
    split_multitap_y_eases(&mut golden, 0, 0.0);
    assert_eq!(golden.len(), 4);
    assert_eq!(golden[0].from.y, None);
    assert_eq!(golden[0].from.x, Some(2.0));
    assert_eq!(
        golden[2].from,
        SongLuaOverlayStateDelta {
            y: Some(10.0),
            ..Default::default()
        }
    );
    assert_eq!(golden[2].easing.as_deref(), Some("outQuad"));
    assert_eq!(golden[3].easing.as_deref(), Some("inQuad"));
    assert_eq!(golden[2].sustain, Some(0.25));
}

thread_local! { static METRIC_CALLS: Cell<(u32, u64)> = const { Cell::new((0, 0)) }; }

fn metric_call(tag: u64) -> u32 {
    METRIC_CALLS.with(|cell| {
        let (count, hash) = cell.get();
        cell.set((count + 1, hash.wrapping_mul(31).wrapping_add(tag)));
        count
    })
}

fn varying_metric_f(_: &str, _: &str, name: &str) -> Option<f32> {
    Some(metric_call(if name.ends_with('X') { 1 } else { 2 }) as f32 / 128.0)
}

fn varying_metric_b(_: &str, _: &str, _: &str) -> Option<bool> {
    Some(metric_call(3) % 5 == 0)
}

#[test]
fn multitap_compile_actor_matches_parent_with_live_resolvers_and_child_baselines() {
    let mut context = SongLuaCompileContext::new(".", "Actor regression");
    let resolver = SongLuaNoteskinResolver {
        metric_f: varying_metric_f,
        metric_b: varying_metric_b,
        ..Default::default()
    };
    for style in ["single", "double"] {
        context.style_name = style.into();
        for taps in [
            vec![8.0],
            vec![8.0, 8.0, 10.0],
            vec![2.0, 1.0, 4.0],
            (0..32).map(|i| i as f32 / 3.0).collect(),
        ] {
            for (lane, peak, skin) in [
                (1, None, "default"),
                (4, Some(2.0), "RaInBoW"),
                (8, Some(-1.0), "é-CyBeR"),
            ] {
                for children in [0, 1, 8] {
                    let desc = MultitapDesc {
                        lane,
                        taps: taps.clone(),
                        peak,
                    };
                    let state = SongLuaOverlayState {
                        visible: false,
                        zoom: 0.9,
                        x: 11.0,
                        y: -13.0,
                        ..Default::default()
                    };
                    let child_states: Vec<_> = (0..children)
                        .map(|i| {
                            (
                                i + 10,
                                SongLuaOverlayState {
                                    visible: i % 2 == 0,
                                    x: i as f32,
                                    diffuse: [0.2, 0.3, 0.4, 0.5],
                                    ..state
                                },
                            )
                        })
                        .collect();
                    let mut old = vec![curve(0)];
                    let mut new = old.clone();
                    METRIC_CALLS.set((0, 0));
                    baseline::push_multitap_actor_eases(
                        &mut old,
                        1,
                        state,
                        2,
                        state,
                        3,
                        state,
                        &child_states,
                        &context,
                        0,
                        resolver,
                        skin,
                        &desc,
                        false,
                    );
                    let calls = METRIC_CALLS.get();
                    METRIC_CALLS.set((0, 0));
                    push_multitap_actor_eases(
                        &mut new,
                        1,
                        state,
                        2,
                        state,
                        3,
                        state,
                        &child_states,
                        &context,
                        0,
                        resolver,
                        skin,
                        &desc,
                        false,
                    );
                    assert_eq!(METRIC_CALLS.get(), calls);
                    assert_eq!(
                        new, old,
                        "style={style}, taps={taps:?}, lane={lane}, children={children}"
                    );
                }
            }
        }
    }
}

#[test]
fn multitap_compile_visibility_and_colors_allocate_nothing() {
    let desc = descriptions(1, false).pop().unwrap();
    crate::perf::assert_no_churn(|| {
        for i in 0..1024 {
            black_box(multitap_is_visible(
                black_box(&desc),
                black_box(i as f32 / 128.0),
            ));
            for name in ["default", "RaInBoW", "unknown", "é-CyBeR"] {
                black_box(multitap_qtzn_color_table(black_box(name)));
            }
        }
    });
}

#[test]
fn multitap_compile_curves_allocate_only_output_easing_names() {
    let mut out = Vec::with_capacity(64);
    out.extend((0..32).map(curve));
    // Only 32 final curve names (at most seven bytes each); no scratch Vec,
    // cloned source names, or reallocations when the caller has output capacity.
    crate::perf::assert_churn_budget(32, 32 * 7, || split_multitap_y_eases(&mut out, 0, 0.0));
    assert_eq!(out.len(), 64);
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn multitap_compile_bench() {
    for count in [1, 8, 32, 128] {
        let desc = MultitapDesc {
            lane: 1,
            taps: (0..count).map(|i| i as f32 * 0.5).collect(),
            peak: None,
        };
        for (position, beat) in [
            ("before", -16.0),
            ("late", (count - 1) as f32 * 0.5 - 0.125),
            ("after", count as f32),
        ] {
            for old in order() {
                crate::perf::measure_sampled(
                    &format!(
                        "visibility_{count}_{position}/{}",
                        if old { "old" } else { "new" }
                    ),
                    512,
                    128,
                    || {
                        for _ in 0..128 {
                            black_box(if old {
                                calc_multitap_phase(black_box(&desc), black_box(beat)).visible
                            } else {
                                multitap_is_visible(black_box(&desc), black_box(beat))
                            });
                        }
                    },
                );
            }
        }
    }
    for (label, name) in [
        ("default", "default".into()),
        ("color", "CoLoR".into()),
        ("rainbow", "RaInBoW".into()),
        ("shadow", "enchantment".into()),
        ("unknown", "unrecognized".into()),
        ("unicode", "雪-é-CyBeR".into()),
        ("short_limit", "x".repeat(16)),
        ("long_limit", "x".repeat(17)),
        ("medium", "x".repeat(64)),
        ("long", format!("{}-NOTE", "unrecognized".repeat(128))),
    ] {
        for old in order() {
            let classify = if old {
                baseline::multitap_qtzn_color_table
            } else {
                multitap_qtzn_color_table
            };
            crate::perf::measure_sampled(
                &format!("colors_{label}/{}", if old { "old" } else { "new" }),
                256,
                128,
                || {
                    for _ in 0..128 {
                        black_box(classify(black_box(&name)));
                    }
                },
            );
        }
    }
    for count in [0, 8, 128, 512] {
        for old in order() {
            let mut out = Vec::with_capacity(count * 2);
            out.extend((0..count).map(curve));
            let split = if old {
                baseline::split_multitap_y_eases
            } else {
                split_multitap_y_eases
            };
            crate::perf::measure_sampled(
                &format!("curves_{count}/{}", if old { "old" } else { "new" }),
                64,
                count,
                || {
                    out.truncate(count);
                    for (i, ease) in out.iter_mut().enumerate() {
                        ease.from.y = Some(10.0);
                        ease.to.y = Some(if i % 2 == 0 { 20.0 } else { 5.0 });
                    }
                    split(black_box(&mut out), 0, 0.0);
                    black_box(&out);
                },
            );
        }
    }
    let context = SongLuaCompileContext::new(".", "Compilation benchmark");
    for count in [1, 16, 128, 512] {
        for overlap in [false, true] {
            let descs = descriptions(count, overlap);
            for old in order() {
                let build = if old {
                    baseline::push_multitap_explosion_eases
                } else {
                    push_multitap_explosion_eases
                };
                crate::perf::measure_sampled(
                    &format!(
                        "explosion_{count}_overlap_{overlap}/{}",
                        if old { "old" } else { "new" }
                    ),
                    16,
                    count,
                    || {
                        let mut out = Vec::new();
                        for lane in 1..=4 {
                            build(
                                &mut out,
                                0,
                                Default::default(),
                                &context,
                                black_box(&descs),
                                lane,
                            );
                        }
                        black_box(out);
                    },
                );
            }
        }
    }
    for count in [1, 8, 32, 128] {
        for children in [0, 4] {
            let desc = MultitapDesc {
                lane: 1,
                taps: (0..count).map(|i| 8.0 + i as f32 * 0.5).collect(),
                peak: Some(2.0),
            };
            let state = SongLuaOverlayState {
                visible: false,
                ..Default::default()
            };
            let child_states: Vec<_> = (0..children)
                .map(|i| (i + 10, SongLuaOverlayState::default()))
                .collect();
            for old in order() {
                let build = if old {
                    baseline::push_multitap_actor_eases
                } else {
                    push_multitap_actor_eases
                };
                crate::perf::measure_sampled(
                    &format!(
                        "actor_{count}_children_{children}/{}",
                        if old { "old" } else { "new" }
                    ),
                    16,
                    count,
                    || {
                        let mut out = Vec::new();
                        build(
                            &mut out,
                            0,
                            state,
                            1,
                            state,
                            2,
                            state,
                            &child_states,
                            &context,
                            0,
                            Default::default(),
                            black_box("default"),
                            black_box(&desc),
                            false,
                        );
                        black_box(out);
                    },
                );
            }
        }
    }
}
