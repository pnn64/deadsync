use super::*;
use deadsync_score::Grade;
use std::hint::black_box;

#[path = "../../../tests/support/kernel_bench.rs"]
mod bench;
#[path = "effects_original.rs"]
#[allow(dead_code, clippy::needless_range_loop)]
mod original;

const GRADES: [Grade; 19] = [
    Grade::Quint,
    Grade::Tier01,
    Grade::Tier02,
    Grade::Tier03,
    Grade::Tier04,
    Grade::Tier05,
    Grade::Tier06,
    Grade::Tier07,
    Grade::Tier08,
    Grade::Tier09,
    Grade::Tier10,
    Grade::Tier11,
    Grade::Tier12,
    Grade::Tier13,
    Grade::Tier14,
    Grade::Tier15,
    Grade::Tier16,
    Grade::Tier17,
    Grade::Failed,
];

fn random(state: &mut u64) -> usize {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state as usize
}

fn panel(seed: u8, mode: usize) -> PanelFrame {
    std::array::from_fn(|index| {
        let led = index / 3;
        if mode == 0
            || (mode == 2 && led % 2 == 0)
            || (mode == 3 && (led * 37 + usize::from(seed)) % 7 < 3)
        {
            0
        } else {
            seed.wrapping_add((index * 37) as u8)
        }
    })
}

fn animations() -> (Arc<FullPadAnim>, Arc<PanelAnim>) {
    let durations: Vec<_> = (0..24)
        .map(|i| if i % 3 == 0 { 0.02 } else { 1.0 / 30.0 })
        .collect();
    let full = Arc::new(FullPadAnim {
        panels: std::array::from_fn(|p| {
            (0..24).map(|f| panel((p * 17 + f * 5) as u8, 1)).collect()
        }),
        durations: durations.clone(),
        loop_frame: 3,
        beats_per_loop: Some(4.0),
    });
    let overlay = Arc::new(PanelAnim {
        frames: (0..24).map(|f| panel((f * 11) as u8, 3)).collect(),
        durations,
        loop_frame: 4,
        loop_end: 18,
    });
    (full, overlay)
}

fn old_drive(drive: OverlayDrive) -> original::OverlayDrive {
    match drive {
        OverlayDrive::OneShot { pressed } => original::OverlayDrive::OneShot { pressed },
        OverlayDrive::Sustain { resume } => original::OverlayDrive::Sustain { resume },
        OverlayDrive::Roll { resume } => original::OverlayDrive::Roll { resume },
    }
}

fn exercise(mut save: impl FnMut(&[u8])) {
    let (full, overlay) = animations();
    let mut live = PanelFx::new();
    let mut old = original::PanelFx::new();
    let mut random_state = 0x7213_aa79_4563_8891;
    for step in 0..4000 {
        let value = random(&mut random_state);
        let pad = value % 4;
        let panel = (value >> 4) % 12;
        let drive = match (value >> 9) % 3 {
            0 => OverlayDrive::OneShot {
                pressed: value & 1 == 0,
            },
            1 => OverlayDrive::Sustain {
                resume: value & 1 == 0,
            },
            _ => OverlayDrive::Roll {
                resume: value & 1 == 0,
            },
        };
        match (value >> 15) % 14 {
            0 => {
                live.play_overlay(pad, panel, overlay.clone(), drive);
                old.play_overlay(pad, panel, overlay.clone(), old_drive(drive));
            }
            1 => {
                live.play_press_overlay(pad, panel, overlay.clone(), drive);
                old.play_press_overlay(pad, panel, overlay.clone(), old_drive(drive));
            }
            2 => {
                live.release_overlay(pad, panel);
                old.release_overlay(pad, panel);
            }
            3 => {
                live.release_press_overlay(pad, panel);
                old.release_press_overlay(pad, panel);
            }
            4 => {
                live.clear_panels_for_pad(pad);
                old.clear_panels_for_pad(pad);
            }
            5 => {
                live.clear_pad(pad);
                old.clear_pad(pad);
            }
            6 => {
                live.set_pad_blackout(pad, value & 1 == 0);
                old.set_pad_blackout(pad, value & 1 == 0);
            }
            7 => {
                live.set_background_for_pad(pad, Some((full.clone(), Clock::Realtime)));
                old.set_background_for_pad(pad, Some((full.clone(), original::Clock::Realtime)));
            }
            8 => {
                let beats_per_loop = [0.0, 4.0, -1.0, f32::INFINITY, f32::NAN][value % 5];
                live.set_background_for_pad(
                    pad,
                    Some((full.clone(), Clock::BeatLocked { beats_per_loop })),
                );
                old.set_background_for_pad(
                    pad,
                    Some((full.clone(), original::Clock::BeatLocked { beats_per_loop })),
                );
            }
            9 => {
                live.set_background_for_pad(pad, None);
                old.set_background_for_pad(pad, None);
            }
            10 if step % 17 == 0 => {
                live.clear_all();
                old.clear_all();
            }
            11 if step % 13 == 0 => {
                live.clear_panels();
                old.clear_panels();
            }
            _ => {}
        }
        let beat = match step % 113 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            _ => (step as f32 - 50.0) / 7.0,
        };
        live.set_beat(beat);
        old.set_beat(beat);
        let dt = [0.0, -0.0, -1.0, f32::NAN, 0.005, 1.0 / 30.0, 0.2][value % 7];
        let wire = *live.tick(dt);
        assert_eq!(wire, *old.tick(dt), "wire at step {step}");
        let state = state_bytes(&live);
        assert_eq!(state, original::state_bytes(&old), "state at step {step}");
        save(&wire);
        save(&state);
    }
}

#[test]
fn engine_events_preserve_wire_and_complete_playback_state() {
    exercise(|_| {});
}

#[test]
fn exceptional_frame_durations_preserve_wrap_release_and_time_bits() {
    for durations in [
        vec![0.0, -0.0, -1.0, 0.03],
        vec![f32::NAN, f32::NEG_INFINITY, 0.01, 0.03],
        vec![0.001, f32::INFINITY, 0.02, 0.03],
    ] {
        for loop_frame in [0, 1, 3, 6] {
            for loop_end in [0, 2, 3, 6] {
                let anim = Arc::new(PanelAnim {
                    frames: (0..4).map(|index| panel(index, 3)).collect(),
                    durations: durations.clone(),
                    loop_frame,
                    loop_end,
                });
                for drive in [
                    OverlayDrive::OneShot { pressed: false },
                    OverlayDrive::OneShot { pressed: true },
                    OverlayDrive::Sustain { resume: false },
                    OverlayDrive::Sustain { resume: true },
                    OverlayDrive::Roll { resume: false },
                    OverlayDrive::Roll { resume: true },
                ] {
                    let mut live = PanelFx::new();
                    let mut old = original::PanelFx::new();
                    live.play_overlay(0, 0, anim.clone(), drive);
                    old.play_overlay(0, 0, anim.clone(), old_drive(drive));
                    for (step, dt) in [0.0, -0.0, f32::NAN, 0.005, 0.2, 0.03, -1.0, 0.2]
                        .into_iter()
                        .enumerate()
                    {
                        if step == 5 {
                            live.release_overlay(0, 0);
                            old.release_overlay(0, 0);
                        }
                        assert_eq!(*live.tick(dt), *old.tick(dt));
                        assert_eq!(state_bytes(&live), original::state_bytes(&old));
                    }
                }
            }
        }
    }
}

#[test]
fn packed_composition_preserves_transparency_and_slice_tails() {
    let mut state = 0x1992_5812_4563_aa77;
    for case in 0..4096 {
        let top = panel(random(&mut state) as u8, case % 4);
        let background = panel(random(&mut state) as u8, 1);
        for (top, background) in [
            (None, None),
            (Some(&top), None),
            (None, Some(&background)),
            (Some(&top), Some(&background)),
        ] {
            let mut live = [123; 75];
            let mut old = live;
            compose_panel(&mut live, top, background);
            original::compose_panel(&mut old, top, background);
            assert_eq!(live, old);
        }
        let mut live = [123; 91];
        let mut old = live;
        let length = case % 92;
        compose_panel(&mut live[..length], Some(&top), Some(&background));
        original::compose_panel(&mut old[..length], Some(&top), Some(&background));
        assert_eq!(live, old);
    }
}

#[test]
fn role_candidates_preserve_order_bytes_and_returned_capacities() {
    let long = "h@rd:非常に難しい\0".repeat(100);
    for grade in GRADES {
        for difficulty in [
            None,
            Some(""),
            Some("hard"),
            Some("edit"),
            Some("e\u{301}"),
            Some(" @ S+ "),
            Some(long.as_str()),
        ] {
            let live = crate::panel_fx::results_role_candidates(grade, difficulty);
            let old = original::results_role_candidates(grade, difficulty);
            assert_eq!(live, old);
            assert_eq!(live.capacity(), old.capacity());
            assert_eq!(
                live.iter().map(String::capacity).collect::<Vec<_>>(),
                old.iter().map(String::capacity).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
#[ignore = "archive wire and playback state trace"]
fn dump_effects_trace() {
    let mut trace = Vec::new();
    exercise(|bytes| trace.extend_from_slice(bytes));
    for grade in GRADES {
        for difficulty in [None, Some(""), Some("hard"), Some("難しい@x")] {
            let roles = crate::panel_fx::results_role_candidates(grade, difficulty);
            trace.extend_from_slice(&(roles.len() as u64).to_le_bytes());
            trace.extend_from_slice(&(roles.capacity() as u64).to_le_bytes());
            for role in roles {
                trace.extend_from_slice(&(role.len() as u64).to_le_bytes());
                trace.extend_from_slice(&(role.capacity() as u64).to_le_bytes());
                trace.extend_from_slice(role.as_bytes());
            }
        }
    }
    std::fs::write(
        std::env::var_os("SMX_EFFECTS_TRACE_OUT").expect("trace path"),
        trace,
    )
    .unwrap();
}

fn compare(label: &str, iterations: usize, mut work: impl FnMut(bool)) {
    bench::compare(label, iterations, &mut work);
    for (variant, current) in [("original", false), ("current", true)] {
        let (_, churn) = crate::lighting_allocations::measure(|| work(current));
        println!("{label}/{variant}: allocation {churn:?}");
    }
}

#[test]
#[ignore = "paired effect engine CPU and allocation benchmark"]
fn benchmark_effects() {
    for (label, mode) in [
        ("compose/black/64", 0),
        ("compose/opaque/64", 1),
        ("compose/alternating/64", 2),
        ("compose/mixed/64", 3),
    ] {
        let top: Vec<_> = (0..64).map(|seed| panel(seed, mode)).collect();
        let background = panel(91, 1);
        compare(label, 1000, |current| {
            let compose = black_box(if current {
                compose_panel as fn(&mut [u8], Option<&PanelFrame>, Option<&PanelFrame>)
            } else {
                original::compose_panel as fn(&mut [u8], Option<&PanelFrame>, Option<&PanelFrame>)
            });
            let mut out = [0; 75];
            for frame in &top {
                compose(
                    black_box(&mut out),
                    Some(black_box(frame)),
                    Some(black_box(&background)),
                );
                black_box(&out);
            }
        });
    }
    let (full, overlay) = animations();
    for (label, dt) in [
        ("advance/quiet/16", 0.005),
        ("advance/normal/16", 1.0 / 30.0),
        ("advance/catchup/16", 0.2),
    ] {
        let mut live = None;
        let mut old = None;
        start_overlay(
            &mut live,
            overlay.clone(),
            OverlayDrive::Sustain { resume: true },
        );
        original::start_overlay(
            &mut old,
            overlay.clone(),
            original::OverlayDrive::Sustain { resume: true },
        );
        compare(label, 2000, |current| {
            if current {
                for _ in 0..16 {
                    advance_overlay(black_box(&mut live), black_box(dt));
                }
                black_box(&live);
            } else {
                for _ in 0..16 {
                    original::advance_overlay(black_box(&mut old), black_box(dt));
                }
                black_box(&old);
            }
        });
    }
    for (label, background, overlays, blackout) in [
        ("tick/idle", false, false, false),
        ("tick/background", true, false, false),
        ("tick/overlays", false, true, false),
        ("tick/mixed", true, true, false),
        ("tick/blackout", true, true, true),
    ] {
        let mut live = PanelFx::new();
        let mut old = original::PanelFx::new();
        for pad in 0..PADS {
            if background {
                live.set_background_for_pad(pad, Some((full.clone(), Clock::Realtime)));
                old.set_background_for_pad(pad, Some((full.clone(), original::Clock::Realtime)));
            }
            if overlays {
                for panel in 0..PANELS {
                    live.play_overlay(
                        pad,
                        panel,
                        overlay.clone(),
                        OverlayDrive::Sustain { resume: true },
                    );
                    old.play_overlay(
                        pad,
                        panel,
                        overlay.clone(),
                        original::OverlayDrive::Sustain { resume: true },
                    );
                    live.play_press_overlay(
                        pad,
                        panel,
                        overlay.clone(),
                        OverlayDrive::Sustain { resume: true },
                    );
                    old.play_press_overlay(
                        pad,
                        panel,
                        overlay.clone(),
                        original::OverlayDrive::Sustain { resume: true },
                    );
                }
            }
            live.set_pad_blackout(pad, blackout);
            old.set_pad_blackout(pad, blackout);
        }
        compare(label, 5000, |current| {
            if current {
                black_box(live.tick(black_box(0.005)));
            } else {
                black_box(old.tick(black_box(0.005)));
            }
        });
    }
    for (label, grade, difficulty) in [
        ("roles/base/diff", Grade::Tier05, Some("challenge")),
        ("roles/plain/diff", Grade::Tier06, Some("hard")),
        ("roles/base/no_diff", Grade::Tier08, None),
    ] {
        compare(label, 1000, |current| {
            black_box(if current {
                crate::panel_fx::results_role_candidates(black_box(grade), black_box(difficulty))
            } else {
                original::results_role_candidates(black_box(grade), black_box(difficulty))
            });
        });
    }
}

fn state_bytes(fx: &PanelFx) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [fx.beat, fx.prev_beat, fx.beat_rate] {
        out.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    for pad in 0..PADS {
        out.push(u8::from(fx.blackout[pad]));
        match &fx.backgrounds[pad] {
            None => out.push(0),
            Some(bg) => {
                out.push(1);
                match bg.clock {
                    Clock::Realtime => out.push(0),
                    Clock::BeatLocked { beats_per_loop } => {
                        out.push(1);
                        out.extend_from_slice(&beats_per_loop.to_bits().to_le_bytes());
                    }
                }
                out.extend_from_slice(&(bg.frame as u64).to_le_bytes());
                out.extend_from_slice(&bg.time_in_frame.to_bits().to_le_bytes());
            }
        }
        for panel in &fx.panels[pad] {
            for slot in [&panel.overlay, &panel.press_overlay] {
                match slot {
                    None => out.push(0),
                    Some(overlay) => {
                        out.extend_from_slice(&[
                            1,
                            u8::from(overlay.sustain),
                            u8::from(overlay.roll),
                            u8::from(overlay.engaged),
                        ]);
                        out.extend_from_slice(&(overlay.frame as u64).to_le_bytes());
                        out.extend_from_slice(&overlay.time_in_frame.to_bits().to_le_bytes());
                    }
                }
            }
        }
    }
    out
}
