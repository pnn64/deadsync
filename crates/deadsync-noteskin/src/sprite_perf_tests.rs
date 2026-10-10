use super::*;
use crate::resource_perf_support as alloc;
use crate::script::SpriteStatePropertiesPlan;
use std::hint::black_box;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired;

// Frozen from starting main a60358c4d, including its borrowed buffer copies.
fn original_slot_plan(
    slot: SpriteSlotPlan,
    frame_count: usize,
    frame_delays: &[f32],
    beat_based: bool,
    sprite_sheet_dims: &mut impl FnMut(&str) -> (u32, u32),
    source_frame_dims: &mut impl FnMut(&str, u32, u32) -> (u32, u32),
) -> Option<SpriteSlotPlan> {
    let SpriteSlotPlan {
        mut def,
        source,
        note_color_translate,
        ..
    } = slot;
    let (texture_key, tex_dims) = match &source {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        }
        | SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            ..
        } => (texture_key.clone(), *tex_dims),
    };
    let (grid_x, grid_y) = sprite_sheet_dims(&texture_key);
    let animation = sprite_state_properties_animation(
        [tex_dims.0, tex_dims.1],
        [grid_x as usize, grid_y as usize],
        def.src,
        frame_count,
        frame_delays,
        beat_based,
    )?;

    def.src = animation.start_src;
    def.size = animation.frame_size;
    let source_frame = source_frame_dims(&texture_key, tex_dims.0, tex_dims.1);
    Some(SpriteSlotPlan {
        def,
        source_size: [source_frame.0 as i32, source_frame.1 as i32],
        source: state_properties_source_plan(
            texture_key,
            tex_dims,
            (grid_x as usize, grid_y as usize),
            animation,
        ),
        note_color_translate,
    })
}

fn slot(animated: bool) -> SpriteSlotPlan {
    let key = "noteskin/example/tap-note 8x8.png".to_owned();
    SpriteSlotPlan {
        def: SpriteDefinition {
            src: [1, 2],
            size: [24, 32],
            ..Default::default()
        },
        source_size: [96, 128],
        source: if animated {
            SpriteSourcePlan::Animated {
                texture_key: key,
                tex_dims: (1024, 1024),
                frame_size: [128, 128],
                grid: (8, 8),
                frame_count: 3,
                frame_indices: Some(vec![2, 1, 0]),
                rate: AnimationRate::FramesPerBeat(3.0),
                frame_durations: Some(vec![0.1, 0.2, 0.3]),
            }
        } else {
            SpriteSourcePlan::Atlas {
                texture_key: key,
                tex_dims: (1024, 1024),
            }
        },
        note_color_translate: true,
    }
}

#[test]
fn owned_state_properties_preserve_results_and_callback_order() {
    for animated in [false, true] {
        for beat in [false, true] {
            for grid in [(0, 0), (1, 1), (8, 8), (16, 4)] {
                for frames in [0, 1, 8, 64, 65, 256] {
                    for delays in [
                        vec![],
                        vec![0.1],
                        vec![-1.0, -0.0, f32::NAN, f32::INFINITY],
                        vec![0.125; 80],
                    ] {
                        let original_calls = std::cell::RefCell::new(Vec::new());
                        let current_calls = std::cell::RefCell::new(Vec::new());
                        let original = original_slot_plan(
                            slot(animated),
                            frames,
                            &delays,
                            beat,
                            &mut |key| {
                                original_calls.borrow_mut().push(format!("sheet:{key}"));
                                grid
                            },
                            &mut |key, x, y| {
                                original_calls
                                    .borrow_mut()
                                    .push(format!("source:{key}:{x}:{y}"));
                                (x / 8, y / 8)
                            },
                        );
                        let current = itg_sprite_animation_slot_plan(
                            slot(animated),
                            SpriteAnimationCommandPlan::StateProperties(
                                SpriteStatePropertiesPlan {
                                    frame_count: frames,
                                    frame_delays: delays,
                                },
                            ),
                            beat,
                            |key| {
                                current_calls.borrow_mut().push(format!("sheet:{key}"));
                                grid
                            },
                            |key, x, y| {
                                current_calls
                                    .borrow_mut()
                                    .push(format!("source:{key}:{x}:{y}"));
                                (x / 8, y / 8)
                            },
                        );
                        if let (Some(a), Some(b)) = (&current, &original) {
                            if let (
                                SpriteSourcePlan::Animated {
                                    frame_durations: Some(a),
                                    ..
                                },
                                SpriteSourcePlan::Animated {
                                    frame_durations: Some(b),
                                    ..
                                },
                            ) = (&a.source, &b.source)
                            {
                                assert!(
                                    a.iter()
                                        .map(|x| x.to_bits())
                                        .eq(b.iter().map(|x| x.to_bits()))
                                );
                            }
                        }
                        assert_eq!(current, original);
                        assert_eq!(current_calls, original_calls);
                    }
                }
            }
        }
    }
}

#[test]
fn state_properties_reuse_texture_and_delay_allocations() {
    let slot = slot(false);
    let SpriteSourcePlan::Atlas { texture_key, .. } = &slot.source else {
        unreachable!()
    };
    let key_pointer = texture_key.as_ptr();
    let delays = vec![0.125; 64];
    let delay_pointer = delays.as_ptr();
    let command = SpriteAnimationCommandPlan::StateProperties(SpriteStatePropertiesPlan {
        frame_count: 64,
        frame_delays: delays,
    });
    let (result, churn) = alloc::measure(|| {
        itg_sprite_animation_slot_plan(slot, command, false, |_| (8, 8), |_, _, _| (128, 128))
    });
    assert_eq!(
        churn,
        alloc::Churn::default(),
        "expected no allocation churn"
    );
    let SpriteSourcePlan::Animated {
        texture_key,
        frame_durations: Some(durations),
        ..
    } = result.unwrap().source
    else {
        unreachable!()
    };
    assert_eq!(texture_key.as_ptr(), key_pointer);
    assert_eq!(durations.as_ptr(), delay_pointer);
}

#[test]
#[ignore = "paired release benchmark"]
fn benchmark_owned_state_properties() {
    for frames in [8, 64, 256, 4096] {
        paired::compare(
            &format!("sprite state properties {frames} frames (including input creation)"),
            20_000,
            |current| {
                let slot = black_box(slot(false));
                let delays = black_box(vec![0.125; frames]);
                let output = if current {
                    itg_sprite_animation_slot_plan(
                        slot,
                        SpriteAnimationCommandPlan::StateProperties(SpriteStatePropertiesPlan {
                            frame_count: frames,
                            frame_delays: delays,
                        }),
                        false,
                        |_| (64, 64),
                        |_, _, _| (16, 16),
                    )
                } else {
                    original_slot_plan(
                        slot,
                        frames,
                        &delays,
                        false,
                        &mut |_| (64, 64),
                        &mut |_, _, _| (16, 16),
                    )
                };
                black_box(output);
            },
        );
    }
}
