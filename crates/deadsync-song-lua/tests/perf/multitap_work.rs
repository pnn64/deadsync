use super::*;
use std::hint::black_box;

#[path = "multitap_work_baseline.rs"]
mod baseline;

fn phase_bits(phase: MultitapPhase) -> ([u32; 3], u8, bool) {
    (
        [phase.pos, phase.squish, phase.lin].map(f32::to_bits),
        phase.qtc,
        phase.visible,
    )
}

fn assert_phase(desc: &MultitapDesc, beat: f32) {
    assert_eq!(
        phase_bits(calc_multitap_phase(desc, beat)),
        phase_bits(baseline::calc_multitap_phase(desc, beat)),
        "taps={:?}, peak={:?}, beat={beat:?}",
        desc.taps,
        desc.peak,
    );
}

#[test]
fn multitap_work_phase_preserves_strict_visibility_and_bounce_apex() {
    let desc = MultitapDesc {
        lane: 1,
        taps: vec![8.0, 10.0, 12.0],
        peak: Some(2.0),
    };
    assert!(!calc_multitap_phase(&desc, 0.0).visible);
    assert!(calc_multitap_phase(&desc, multitap_visible_start(8.0)).visible);
    let apex = calc_multitap_phase(&desc, 9.0);
    assert_eq!(apex.pos, 0.5);
    assert_eq!(apex.lin, 0.5);
    assert_eq!(apex.squish, -0.1);
    assert!(calc_multitap_phase(&desc, 12.0).visible);
    assert!(!calc_multitap_phase(&desc, 12.0_f32.next_up()).visible);
}

#[test]
fn multitap_work_phase_matches_parent_at_boundaries_duplicates_and_random_beats() {
    let mut rng = 31_u64;
    for count in [1, 2, 3, 8, 32, 128] {
        for seed in 0..32 {
            let mut taps = vec![-4.0 + seed as f32 / 48.0];
            for _ in 1..count {
                rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                let gap = [0.0, f32::EPSILON, 1.0 / 48.0, 0.25, 1.0, 8.0][(rng >> 32) as usize % 6];
                taps.push(taps.last().unwrap() + gap);
            }
            for peak in [None, Some(-0.0), Some(0.5), Some(4.0)] {
                let desc = MultitapDesc {
                    lane: 1,
                    taps: taps.clone(),
                    peak,
                };
                for &tap in &taps {
                    for beat in [tap.next_down(), tap, tap.next_up(), tap - 8.0, tap - 7.0] {
                        assert_phase(&desc, beat);
                    }
                }
                for _ in 0..32 {
                    rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let fraction = (rng >> 32) as f32 / u32::MAX as f32;
                    assert_phase(&desc, -16.0 + fraction * (taps.last().unwrap() + 32.0));
                }
                for beat in [f32::NEG_INFINITY, f32::INFINITY, f32::NAN] {
                    assert_phase(&desc, beat);
                }
            }
        }
    }
    // Public callers can supply unsorted taps; keep the existing early-stop behavior.
    for taps in [vec![2.0, 1.0, 4.0], vec![-0.0, 0.0, 0.5, 0.5, 1.0]] {
        for peak in [None, Some(2.0)] {
            let desc = MultitapDesc {
                lane: 1,
                taps: taps.clone(),
                peak,
            };
            for i in -8..48 {
                assert_phase(&desc, i as f32 / 8.0);
            }
        }
    }
}

fn sample(index: usize) -> (f32, SongLuaOverlayState) {
    (
        index as f32 * 0.25,
        SongLuaOverlayState {
            visible: index % 7 >= 2,
            x: index as f32 * 2.0,
            y: -(index as f32),
            diffuse: [0.25, 0.5, 0.75, 0.5],
            ..SongLuaOverlayState::default()
        },
    )
}

#[test]
fn multitap_work_stream_preserves_adjacent_samples_and_existing_output() {
    let state = SongLuaOverlayState::default();
    for count in [0, 1, 2, 3, 32, 1024] {
        let mut samples: Vec<_> = (0..count).map(sample).collect();
        for irregular in [false, true] {
            if irregular {
                for (index, (beat, _)) in samples.iter_mut().enumerate() {
                    if index % 5 == 0 {
                        *beat = -0.0;
                    }
                }
            }
            let mut old = Vec::new();
            let mut new = Vec::new();
            // Repeated calls must append and must reset the previous-sample state.
            for _ in 0..2 {
                baseline::push_overlay_sample_eases(&mut old, 4, state, &samples);
                push_overlay_sample_eases_iter(&mut new, 4, state, samples.iter().copied());
                assert_eq!(new, old);
            }
        }
    }
}

fn descs(count: usize, overlapping: bool) -> Vec<MultitapDesc> {
    (0..count)
        .map(|i| {
            let start = if overlapping {
                i as f32 * 0.125
            } else {
                i as f32 * 16.0
            };
            MultitapDesc {
                lane: 1 + i % 4,
                taps: (0..8).map(|tap| start + tap as f32 * 0.5).collect(),
                peak: if i % 2 == 0 { None } else { Some(1.5) },
            }
        })
        .collect()
}

#[test]
fn multitap_work_explosions_match_parent_across_lanes_overlap_and_empty_inputs() {
    let mut context = SongLuaCompileContext::new(".", "Multitap test");
    for style in ["single", "double", "versus"] {
        context.style_name = style.into();
        for count in [0, 1, 2, 16, 128] {
            for overlapping in [false, true] {
                let descs = descs(count, overlapping);
                for lane in 1..=8 {
                    for visible in [false, true] {
                        let state = SongLuaOverlayState {
                            visible,
                            x: 23.0,
                            zoom: 0.8,
                            ..Default::default()
                        };
                        let mut old = Vec::new();
                        let mut new = Vec::new();
                        baseline::push_multitap_explosion_eases(
                            &mut old, 3, state, &context, &descs, lane,
                        );
                        push_multitap_explosion_eases(&mut new, 3, state, &context, &descs, lane);
                        assert_eq!(new, old, "style={style}, count={count}, lane={lane}");
                    }
                }
            }
        }
    }
}

#[test]
fn multitap_work_phase_and_unchanged_sample_stream_have_no_churn() {
    let desc = descs(1, false).pop().unwrap();
    let state = SongLuaOverlayState {
        visible: false,
        ..Default::default()
    };
    let mut out = Vec::new();
    crate::perf::assert_no_churn(|| {
        for i in 0..1024 {
            black_box(calc_multitap_phase(&desc, black_box(i as f32 / 256.0)));
        }
        push_overlay_sample_eases_iter(&mut out, 0, state, (0..4096).map(|i| (i as f32, state)));
    });
    assert!(out.is_empty());
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn multitap_work_bench() {
    for count in [1, 2, 8, 32, 128] {
        for peak in [None, Some(2.0)] {
            let desc = MultitapDesc {
                lane: 1,
                taps: (0..count).map(|i| i as f32 * 0.5).collect(),
                peak,
            };
            for (position, beat) in [
                ("before", -16.0),
                ("late", (count - 1) as f32 * 0.5 - 0.125),
                ("after", count as f32),
            ] {
                assert_phase(&desc, beat);
                for old in order() {
                    let phase = if old {
                        baseline::calc_multitap_phase
                    } else {
                        calc_multitap_phase
                    };
                    crate::perf::measure_sampled(
                        &format!(
                            "phase_{count}_peak_{}_{position}/{}",
                            peak.is_some(),
                            if old { "old" } else { "new" }
                        ),
                        512,
                        128,
                        || {
                            for _ in 0..128 {
                                black_box(phase(black_box(&desc), black_box(beat)));
                            }
                        },
                    );
                }
            }
        }
    }
    // This comparison isolates streaming from phase calculation: identical generated states.
    for count in [0, 1, 16, 256, 1024] {
        for old in order() {
            crate::perf::measure_sampled(
                &format!("sample_stream_{count}/{}", if old { "old" } else { "new" }),
                32,
                count,
                || {
                    let mut out = Vec::new();
                    if old {
                        let samples: Vec<_> = (0..black_box(count)).map(sample).collect();
                        baseline::push_overlay_sample_eases(
                            &mut out,
                            0,
                            Default::default(),
                            &samples,
                        );
                    } else {
                        push_overlay_sample_eases_iter(
                            &mut out,
                            0,
                            Default::default(),
                            (0..black_box(count)).map(sample),
                        );
                    }
                    black_box(out);
                },
            );
        }
    }
    let context = SongLuaCompileContext::new(".", "Multitap benchmark");
    for count in [1, 16, 128, 512] {
        for overlapping in [false, true] {
            let descs = descs(count, overlapping);
            for old in order() {
                let build = if old {
                    baseline::push_multitap_explosion_eases
                } else {
                    push_multitap_explosion_eases
                };
                crate::perf::measure_sampled(
                    &format!(
                        "explosion_{count}_overlap_{overlapping}/{}",
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
}

fn order() -> [bool; 2] {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        [false, true]
    } else {
        [true, false]
    }
}
