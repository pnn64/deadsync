use super::*;
use crate::perf::{assert_churn_budget, measure_sampled};
use std::hint::black_box;

#[path = "sprite_setup/baseline.rs"]
mod baseline;

const KEY: &str = "noteskins/dance/benchmark/Down Tap Note 8x8.png";

fn all_frames(old: bool, grid: [u32; 2], delay: Option<f32>) -> Option<SpriteSlotPlan> {
    let build = if old {
        baseline::itg_all_frames_sprite_slot_plan_from_path
    } else {
        itg_all_frames_sprite_slot_plan_from_path
    };
    build(
        Path::new(KEY),
        delay,
        false,
        |_: &Path| Some(KEY.to_owned()),
        |_: &str| Some((512, 512)),
        |_: &str| (grid[0], grid[1]),
        |_: &str, _: u32, _: u32| (64, 64),
    )
}

fn explicit(old: bool, count: usize, grid: [u32; 2]) -> Option<SpriteSlotPlan> {
    let build = if old {
        baseline::itg_animation_sprite_slot_plan_from_path
    } else {
        itg_animation_sprite_slot_plan_from_path
    };
    build(
        Path::new(KEY),
        3,
        count,
        Some(&[2, 7, 1]),
        Some(&[0.25, 0.5]),
        true,
        |_: &Path| Some(KEY.to_owned()),
        |_: &str| Some((512, 512)),
        |_: &str| (grid[0], grid[1]),
        |_: &str, _: u32, _: u32| (64, 64),
    )
}

#[test]
fn uniform_animation_matches_old_values_and_playback() {
    for grid in [[0, 0], [1, 1], [0, 8], [4, 2], [8, 8], [64, 64]] {
        for delay in [
            None,
            Some(-1.0),
            Some(-0.0),
            Some(0.0),
            Some(0.125),
            Some(f32::NAN),
            Some(f32::INFINITY),
            Some(f32::NEG_INFINITY),
        ] {
            for beat_based in [false, true] {
                let old =
                    baseline::sprite_all_frames_animation_plan([513, 257], grid, delay, beat_based);
                let new = sprite_all_frames_animation_plan([513, 257], grid, delay, beat_based);
                assert_eq!(new, old, "grid={grid:?}, delay={delay:?}");
                if let (Some(new), Some(old)) = (new, old) {
                    for time in [-2.0, -0.0, 0.0, 0.125, 1.0, 513.5, f32::INFINITY, f32::NAN] {
                        let frame = |plan: &SpriteAnimationPlan| {
                            sprite_frame_index(
                                plan.frame_count,
                                plan.rate,
                                plan.frame_durations.as_deref(),
                                time,
                                time * 1.75,
                            )
                        };
                        assert_eq!(frame(&new), frame(&old));
                        assert_eq!(
                            new.frame_durations
                                .as_ref()
                                .map(|v| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
                            old.frame_durations
                                .as_ref()
                                .map(|v| v.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn path_plans_match_old_animation_and_atlas_fallbacks() {
    for grid in [[0, 0], [1, 1], [0, 4], [8, 8]] {
        for count in [0, 1, 2, 9, 64, 257] {
            assert_eq!(explicit(false, count, grid), explicit(true, count, grid));
        }
        for delay in [None, Some(0.0), Some(0.1), Some(f32::NAN)] {
            assert_eq!(
                all_frames(false, grid, delay),
                all_frames(true, grid, delay)
            );
        }
    }
}

#[test]
fn path_plans_preserve_callback_order_and_early_returns() {
    use std::cell::RefCell;
    for missing in 0..3 {
        let run = |old| {
            let calls = RefCell::new(Vec::new());
            let build = if old {
                baseline::itg_all_frames_sprite_slot_plan_from_path
            } else {
                itg_all_frames_sprite_slot_plan_from_path
            };
            let plan = build(
                Path::new(KEY),
                Some(0.25),
                true,
                |_: &Path| {
                    calls.borrow_mut().push(0);
                    (missing != 0).then(|| KEY.to_owned())
                },
                |_: &str| {
                    calls.borrow_mut().push(1);
                    (missing != 1).then_some((512, 512))
                },
                |_: &str| {
                    calls.borrow_mut().push(2);
                    (8, 8)
                },
                |_: &str, _: u32, _: u32| {
                    calls.borrow_mut().push(3);
                    (64, 64)
                },
            );
            (plan, calls.into_inner())
        };
        assert_eq!(run(false), run(true));
    }
}

#[test]
fn uniform_plans_allocate_only_the_output_key_and_durations() {
    assert_churn_budget(1, 64 * size_of::<f32>(), || {
        black_box(sprite_all_frames_animation_plan(
            [512, 512],
            [8, 8],
            Some(0.25),
            false,
        ));
    });
    assert_churn_budget(2, KEY.len() + 64 * size_of::<f32>(), || {
        black_box(all_frames(false, [8, 8], Some(0.25)));
    });
    assert_churn_budget(1, KEY.len(), || {
        black_box(all_frames(false, [1, 1], Some(0.25)));
    });
}

#[test]
#[ignore = "manual release old/new benchmark"]
fn preparation_bench_sprite() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, grid, delay, count) in [
        ("sprite_atlas", [1, 1], Some(0.25), None),
        ("sprite_uniform_8", [4, 2], Some(0.25), None),
        ("sprite_uniform_64", [8, 8], Some(0.25), None),
        ("sprite_uniform_4096", [64, 64], Some(0.25), None),
        ("sprite_no_delays", [8, 8], None, None),
        ("sprite_explicit_64", [8, 8], None, Some(64)),
        ("sprite_explicit_fallback", [1, 1], None, Some(64)),
    ] {
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("{name}_{}", if old { "old" } else { "new" });
            measure_sampled(&label, 2048, 1, || match count {
                Some(count) => explicit(old, black_box(count), black_box(grid)),
                None => all_frames(old, black_box(grid), black_box(delay)),
            });
        }
    }
}
