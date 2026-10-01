use super::*;
use crate::perf;
use deadsync_noteskin::script::{SpriteAnimationCommandPlan as Command, SpriteStatePropertiesPlan};
use std::hint::black_box;

mod baseline;

fn plan(key: &str, count: Option<usize>, indexed: bool) -> SpriteSlotPlan {
    let mut plan = if let Some(count) = count {
        generated_animation_sprite_slot_plan(
            key.to_owned(),
            (512, 256),
            [64, 64],
            count,
            AnimationRate::FramesPerSecond(30.0),
            false,
        )
    } else {
        deadsync_noteskin::atlas_sprite_slot_plan(key.to_owned(), (512, 256), (64, 64), true)
    };
    plan.def.src = [37, 79];
    plan.def.mirror_h = true;
    if let SpriteSourcePlan::Animated {
        grid,
        frame_indices,
        frame_durations,
        ..
    } = &mut plan.source
    {
        *grid = (8, 4);
        *frame_indices = indexed.then(|| vec![7, 2, 11, 0]);
        *frame_durations = Some(vec![0.125, 0.25, -0.0, f32::NAN]);
    }
    plan
}

fn fixture(key: &str, count: Option<usize>, indexed: bool) -> SpriteSlot {
    let mut slot = slot_from_plan(plan(key, count, indexed));
    slot.animation_start_frame = 2;
    slot.animation_start_time = 0.375;
    slot.uv_offset = [0.125, -0.25];
    slot.uv_velocity = [0.03, -0.04];
    let _ = assets::sprite_sheet_dims(slot.texture_key());
    let _ = assets::texture_source_frame_dims_from_real(slot.texture_key(), 512, 256);
    slot
}

fn snapshot(slot: &SpriteSlot) -> String {
    format!("{slot:?}").replacen(
        &format!("stable_id: {}", slot.stable_id()),
        "stable_id: <identity>",
        1,
    )
}

fn compare(old: &SpriteSlot, new: &SpriteSlot) {
    assert_eq!(snapshot(old), snapshot(new));
    match (old.source.as_ref(), new.source.as_ref()) {
        (
            SpriteSource::Animated {
                rate: a,
                frame_durations: ad,
                frame_indices: ai,
                ..
            },
            SpriteSource::Animated {
                rate: b,
                frame_durations: bd,
                frame_indices: bi,
                ..
            },
        ) => {
            let bits = |rate: AnimationRate| match rate {
                AnimationRate::FramesPerSecond(v) => (false, v.to_bits()),
                AnimationRate::FramesPerBeat(v) => (true, v.to_bits()),
            };
            assert_eq!(bits(*a), bits(*b));
            assert_eq!(ai, bi);
            assert_eq!(
                ad.as_deref()
                    .map(|d| d.iter().map(|v| v.to_bits()).collect::<Vec<_>>()),
                bd.as_deref()
                    .map(|d| d.iter().map(|v| v.to_bits()).collect::<Vec<_>>())
            );
        }
        (SpriteSource::Atlas { .. }, SpriteSource::Atlas { .. }) => {}
        _ => panic!("source variants differ"),
    }
    for time in [
        -10.0,
        -0.0,
        0.0,
        0.125,
        0.5,
        1.0,
        10.0,
        f32::NAN,
        f32::INFINITY,
    ] {
        assert_eq!(
            old.frame_index(time, time * 3.0),
            new.frame_index(time, time * 3.0)
        );
        assert_eq!(
            old.frame_index_from_phase(time),
            new.frame_index_from_phase(time)
        );
        for frame in [0, 1, 7, new.source.frame_count().saturating_sub(1)] {
            assert_eq!(
                old.uv_for_frame_at(frame, time).map(f32::to_bits),
                new.uv_for_frame_at(frame, time).map(f32::to_bits)
            );
        }
    }
}

fn state(count: usize, delays: &[f32]) -> Command {
    Command::StateProperties(SpriteStatePropertiesPlan {
        frame_count: count,
        frame_delays: delays.to_vec(),
    })
}

#[test]
fn shared_empty_defaults_preserve_complete_slot_state_and_distinct_cache_ids() {
    for count in [None, Some(1), Some(7), Some(64), Some(129)] {
        for indexed in [false, true] {
            let declaration = plan("sprite-init 8x4.png", count, indexed);
            let old = baseline::slot_from_plan(declaration.clone());
            let new = slot_from_plan(declaration);
            compare(&old, &new);
            assert_ne!(old.stable_id(), new.stable_id());
            let other = slot_from_plan(plan("sprite-init 8x4.png", count, indexed));
            assert!(Arc::ptr_eq(&new.model_timeline, &other.model_timeline));
            assert!(Arc::ptr_eq(
                &new.model_auto_rot_z_keys,
                &other.model_auto_rot_z_keys
            ));
        }
    }
}

#[test]
fn direct_uniform_delays_preserve_float_bits_indices_timing_and_uvs() {
    for count in [0, 1, 2, 7, 32, 64, 65, 129] {
        for indexed in 0..3 {
            let mut initial = fixture("sprite-init 8x4.png", Some(count), indexed == 2);
            if indexed == 1 {
                if let SpriteSource::Animated { frame_indices, .. } =
                    Arc::get_mut(&mut initial.source).unwrap()
                {
                    *frame_indices = Some(Arc::clone(&SEQUENTIAL_FRAME_INDICES));
                }
            }
            for delay in [
                -1.0,
                -0.0,
                0.0,
                f32::from_bits(1),
                1e-6,
                0.125,
                0.1,
                2.0,
                f32::NAN,
                f32::INFINITY,
                f32::NEG_INFINITY,
            ] {
                for beat in [false, true] {
                    let mut old = initial.clone();
                    let mut new = initial.clone();
                    baseline::itg_apply_sprite_animation_plan(
                        &mut old,
                        Command::AllStateDelays(delay),
                        beat,
                    );
                    itg_apply_sprite_animation_plan(&mut new, Command::AllStateDelays(delay), beat);
                    compare(&old, &new);
                    assert!(Arc::ptr_eq(
                        &initial.texture_key_shared(),
                        &new.texture_key_shared()
                    ));
                    if let (
                        SpriteSource::Animated {
                            frame_indices: Some(a),
                            ..
                        },
                        SpriteSource::Animated {
                            frame_indices: Some(b),
                            ..
                        },
                    ) = (initial.source.as_ref(), new.source.as_ref())
                    {
                        assert!(Arc::ptr_eq(a, b));
                    }
                }
            }
        }
    }
}

#[test]
fn direct_state_properties_preserve_sheet_clamping_origins_delay_rules_and_uvs() {
    for key in [
        "sprite-init.png",
        "sprite-init 1x1.png",
        "sprite-init 4x2.png",
        "sprite-init 8x8 (doubleres).png",
    ] {
        for animated in [false, true] {
            for src in [[0, 0], [37, 79], [-3, -7], [999, 999]] {
                let mut initial = fixture(key, animated.then_some(7), true);
                initial.def.src = src;
                for count in [0, 1, 2, 3, 7, 64, 129] {
                    for delays in [
                        &[][..],
                        &[0.125][..],
                        &[0.0, -0.0, -1.0, 0.1][..],
                        &[
                            f32::NAN,
                            f32::INFINITY,
                            f32::NEG_INFINITY,
                            f32::from_bits(1),
                        ][..],
                    ] {
                        for beat in [false, true] {
                            let mut old = initial.clone();
                            let mut new = initial.clone();
                            baseline::itg_apply_sprite_animation_plan(
                                &mut old,
                                state(count, delays),
                                beat,
                            );
                            itg_apply_sprite_animation_plan(&mut new, state(count, delays), beat);
                            compare(&old, &new);
                            assert!(Arc::ptr_eq(
                                &initial.texture_key_shared(),
                                &new.texture_key_shared()
                            ));
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn direct_commands_preserve_unique_shared_and_weak_source_owners() {
    for command in [Command::AllStateDelays(0.125), state(7, &[0.125, 0.25])] {
        for mode in 0..3 {
            let mut new = fixture("sprite-init 8x4.png", Some(7), true);
            if let SpriteSource::Animated {
                cached_handle,
                cached_generation,
                cached_actor_texture,
                ..
            } = new.source.as_ref()
            {
                cached_handle.store(17, Ordering::Relaxed);
                cached_generation.store(29, Ordering::Relaxed);
                cached_actor_texture.store(83, Ordering::Relaxed);
            }
            let pointer = Arc::as_ptr(&new.source);
            let mut old = new.clone();
            baseline::itg_apply_sprite_animation_plan(&mut old, command.clone(), true);
            let retained = (mode == 1).then(|| Arc::clone(&new.source));
            let weak = (mode == 2).then(|| Arc::downgrade(&new.source));
            let before = format!("{:?}", new.source);
            itg_apply_sprite_animation_plan(&mut new, command.clone(), true);
            compare(&old, &new);
            if mode == 0 {
                assert_eq!(pointer, Arc::as_ptr(&new.source));
            } else {
                assert_ne!(pointer, Arc::as_ptr(&new.source));
            }
            if let Some(owner) = retained {
                assert_eq!(before, format!("{owner:?}"));
            }
            if let Some(weak) = weak {
                assert!(weak.upgrade().is_none());
            }
        }
    }
}

#[test]
fn model_and_static_uniform_commands_remain_noops_without_allocator_churn() {
    for model in [false, true] {
        let mut slot = fixture("sprite-init.png", None, false);
        if model {
            slot.model = test_model_slot().model;
        }
        let before = snapshot(&slot);
        let source = Arc::clone(&slot.source);
        perf::assert_no_churn(|| {
            itg_apply_sprite_animation_plan(&mut slot, Command::AllStateDelays(0.125), false)
        });
        if model {
            itg_apply_sprite_animation_plan(&mut slot, state(7, &[0.125]), true);
        }
        assert_eq!(before, snapshot(&slot));
        assert!(Arc::ptr_eq(&source, &slot.source));
    }
}

#[test]
fn command_sequences_and_final_freezing_match_parent() {
    for script in [
        "SetStateProperties,Sprite.LinearFrames(7,0.875);SetAllStateDelays,0.25;setstate,3;pause",
        "SetAllStateDelays,0.1;SetStateProperties,Sprite.LinearFrames(3,0.75);play",
        "SetStateProperties,Sprite.LinearFrames(1,1);SetAllStateDelays,0;SetSecondsIntoAnimation,0.75;pause",
    ] {
        for animated in [false, true] {
            let initial = fixture("sprite-init 8x4.png", animated.then_some(7), true);
            let mut old = initial.clone();
            let mut new = initial.clone();
            apply_sprite_animation_script_plans(&mut old, script, true, |slot, command, beat| {
                baseline::itg_apply_sprite_animation_plan(slot, command, beat)
            });
            apply_sprite_animation_script_plans(
                &mut new,
                script,
                true,
                itg_apply_sprite_animation_plan,
            );
            let commands = HashMap::from([("initcommand".to_owned(), script.to_owned())]);
            itg_apply_initial_sprite_state(&mut old, &commands);
            itg_apply_initial_sprite_state(&mut new, &commands);
            compare(&old, &new);
        }
    }
}

#[test]
fn defaults_and_direct_commands_reduce_complete_owning_allocation_churn() {
    let declaration = plan("sprite-init 8x4.png", Some(32), true);
    let _ = slot_from_plan(declaration.clone());
    perf::assert_reduced_churn(
        || {
            black_box(baseline::slot_from_plan(declaration.clone()));
        },
        || {
            black_box(slot_from_plan(declaration.clone()));
        },
    );
    for count in [7, 64, 65, 129] {
        let initial = fixture("sprite-init 8x4.png", Some(count), true);
        for command in [Command::AllStateDelays(0.125), state(count, &[0.125, 0.25])] {
            perf::assert_reduced_churn(
                || {
                    let mut slot = initial.clone();
                    baseline::itg_apply_sprite_animation_plan(&mut slot, command.clone(), true);
                    black_box(slot);
                },
                || {
                    let mut slot = initial.clone();
                    itg_apply_sprite_animation_plan(&mut slot, command.clone(), true);
                    black_box(slot);
                },
            );
        }
        perf::assert_churn_budget(
            if count <= 64 { 2 } else { 3 },
            264 + 32 + count * 8,
            || {
                let mut slot = initial.clone();
                itg_apply_sprite_animation_plan(&mut slot, Command::AllStateDelays(0.125), true);
                black_box(slot);
            },
        );
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_sprite_initialization() {
    for (name, count, indexed) in [
        ("atlas", None, false),
        ("animated", Some(32), false),
        ("indexed", Some(64), true),
    ] {
        let declaration = plan("sprite-init 8x4.png", count, indexed);
        compare(
            &baseline::slot_from_plan(declaration.clone()),
            &slot_from_plan(declaration.clone()),
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("init_defaults_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let plan = black_box(&declaration).clone();
                    black_box(if new {
                        slot_from_plan(plan)
                    } else {
                        baseline::slot_from_plan(plan)
                    });
                }
            })
        });
    }
    for (name, key, count, indexed, command) in [
        (
            "uniform_small",
            "sprite-init 8x4.png".to_owned(),
            Some(7),
            false,
            Command::AllStateDelays(0.125),
        ),
        (
            "uniform_indexed",
            "sprite-init 8x4.png".to_owned(),
            Some(64),
            true,
            Command::AllStateDelays(0.125),
        ),
        (
            "uniform_large",
            "sprite-init 8x4.png".to_owned(),
            Some(129),
            true,
            Command::AllStateDelays(0.125),
        ),
        (
            "uniform_static",
            "sprite-init 8x4.png".to_owned(),
            None,
            false,
            Command::AllStateDelays(0.125),
        ),
        (
            "state_atlas",
            "sprite-init 8x4.png".to_owned(),
            None,
            false,
            state(32, &[0.125, 0.25]),
        ),
        (
            "state_indexed",
            "sprite-init 8x4.png".to_owned(),
            Some(64),
            true,
            state(32, &[0.125, 0.25]),
        ),
        (
            "state_long_key",
            format!("{} 8x4.png", "k".repeat(1024)),
            Some(64),
            true,
            state(32, &[0.125, 0.25]),
        ),
        (
            "state_static",
            "sprite-init.png".to_owned(),
            None,
            false,
            state(32, &[0.125, 0.25]),
        ),
        (
            "state_one",
            "sprite-init 8x4.png".to_owned(),
            Some(64),
            true,
            state(1, &[0.125]),
        ),
    ] {
        let initial = fixture(&key, count, indexed);
        let mut old = initial.clone();
        let mut new = initial.clone();
        baseline::itg_apply_sprite_animation_plan(&mut old, command.clone(), true);
        itg_apply_sprite_animation_plan(&mut new, command.clone(), true);
        compare(&old, &new);
        pairs(|label, new| {
            perf::measure_sampled(&format!("init_command_{name}_{label}"), 256, 64, || {
                for _ in 0..64 {
                    let mut slot = black_box(&initial).clone();
                    let command = black_box(&command).clone();
                    if new {
                        itg_apply_sprite_animation_plan(&mut slot, command, true);
                    } else {
                        baseline::itg_apply_sprite_animation_plan(&mut slot, command, true);
                    }
                    black_box(slot);
                }
            })
        });
    }
    for (name, command) in [
        ("uniform", Command::AllStateDelays(0.125)),
        ("state", state(32, &[0.125, 0.25])),
    ] {
        pairs(|label, new| {
            perf::measure_sampled_with_setup(
                &format!("init_unique_{name}_{label}"),
                128,
                64,
                || {
                    (0..64)
                        .map(|_| fixture("sprite-init 8x4.png", Some(64), true))
                        .collect::<Vec<_>>()
                },
                |slots| {
                    for slot in slots {
                        if new {
                            itg_apply_sprite_animation_plan(
                                black_box(slot),
                                black_box(&command).clone(),
                                true,
                            );
                        } else {
                            baseline::itg_apply_sprite_animation_plan(
                                black_box(slot),
                                black_box(&command).clone(),
                                true,
                            );
                        }
                    }
                },
            )
        });
    }
    let mut model = fixture("sprite-init 8x4.png", Some(64), true);
    model.model = test_model_slot().model;
    pairs(|label, new| {
        perf::measure_sampled(&format!("init_model_noop_{label}"), 256, 64, || {
            for _ in 0..64 {
                let mut slot = black_box(&model).clone();
                if new {
                    itg_apply_sprite_animation_plan(
                        &mut slot,
                        Command::AllStateDelays(0.125),
                        true,
                    );
                } else {
                    baseline::itg_apply_sprite_animation_plan(
                        &mut slot,
                        Command::AllStateDelays(0.125),
                        true,
                    );
                }
                black_box(slot);
            }
        })
    });
}
