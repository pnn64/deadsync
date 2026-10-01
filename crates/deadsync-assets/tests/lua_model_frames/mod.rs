use super::*;
use crate::{
    noteskin::{SpriteSlot, SpriteSource, test_model_slot},
    perf,
};
use deadsync_noteskin::{
    AnimationRate, SpriteAnimatedUvCache, SpriteAtlasUvCache, SpriteDefinition, SpriteFrameTiming,
    SpriteSourcePlan,
};
use std::{hint::black_box, sync::Arc};

mod baseline;

fn texture(count: Option<usize>, indexed: bool, mixed: bool) -> SpriteSlot {
    let mut slot = test_model_slot();
    slot.model = None;
    slot.def = SpriteDefinition {
        src: [7, 11],
        size: [7, 11],
        mirror_h: true,
        mirror_v: true,
        rotation_deg: 37,
    };
    slot.custom_uv = mixed.then_some([0.2, 0.8, -0.25, 1.0]);
    slot.uv_velocity = [0.25, -0.5];
    slot.uv_offset = [0.125, -0.25];
    slot.source = if let Some(count) = count {
        let mut plan = deadsync_noteskin::generated_animation_sprite_slot_plan(
            "lua-frames".into(),
            (56, 44),
            [7, 11],
            count,
            AnimationRate::FramesPerSecond(8.0),
            false,
        );
        plan.def = slot.def.clone();
        if let deadsync_noteskin::SpriteSourcePlan::Animated {
            frame_indices,
            frame_durations,
            grid,
            ..
        } = &mut plan.source
        {
            *grid = (8, 4);
            *frame_indices = indexed.then(|| vec![7, 3, 11]);
            *frame_durations = Some(
                (0..count.saturating_sub(2))
                    .map(|n| {
                        if mixed {
                            [
                                0.0,
                                -0.0,
                                -0.125,
                                0.3,
                                f32::NAN,
                                f32::INFINITY,
                                f32::NEG_INFINITY,
                            ][n % 7]
                        } else {
                            0.125
                        }
                    })
                    .collect(),
            );
        }
        source(plan.source, &slot.def)
    } else {
        source(
            deadsync_noteskin::SpriteSourcePlan::Atlas {
                texture_key: "lua-atlas".into(),
                tex_dims: (56, 44),
            },
            &slot.def,
        )
    };
    slot
}

fn source(plan: SpriteSourcePlan, def: &SpriteDefinition) -> Arc<SpriteSource> {
    use std::sync::atomic::AtomicU64;
    let texel_scale = [1.0 / 56.0, 1.0 / 44.0];
    Arc::new(match plan {
        SpriteSourcePlan::Atlas {
            texture_key,
            tex_dims,
        } => SpriteSource::Atlas {
            texture_key: texture_key.into(),
            tex_dims,
            texel_scale,
            uv_cache: SpriteAtlasUvCache::new(texel_scale, def),
            cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
            cached_generation: AtomicU64::new(u64::MAX),
            cached_actor_texture: AtomicU64::new(0),
        },
        SpriteSourcePlan::Animated {
            texture_key,
            tex_dims,
            frame_size,
            grid,
            frame_count,
            frame_indices,
            rate,
            frame_durations,
        } => {
            let frame_timing = frame_durations
                .as_deref()
                .map(|d| SpriteFrameTiming::new(frame_count, d));
            let uv_cache = SpriteAnimatedUvCache::new(
                texel_scale,
                def,
                frame_size,
                [grid.0, grid.1],
                frame_count,
                frame_indices.is_some(),
            );
            SpriteSource::Animated {
                texture_key: texture_key.into(),
                tex_dims,
                texel_scale,
                frame_size,
                grid,
                frame_count,
                frame_indices: frame_indices.map(Arc::from),
                rate,
                frame_durations: frame_durations.map(Arc::from),
                frame_timing,
                uv_cache,
                cached_handle: AtomicU64::new(deadlib_render_core::INVALID_TEXTURE_HANDLE),
                cached_generation: AtomicU64::new(u64::MAX),
                cached_actor_texture: AtomicU64::new(0),
            }
        }
    })
}

fn compare_frames(a: &ModelAdditiveFrames, b: &ModelAdditiveFrames) {
    assert_eq!(a.len(), b.len());
    for ((au, at), (bu, bt)) in a.iter().zip(b.iter()) {
        assert_eq!(au.map(f32::to_bits), bu.map(f32::to_bits));
        assert_eq!(at.to_bits(), bt.to_bits());
    }
}

fn compare_layers(a: &Option<SongLuaOverlayModelLayer>, b: &Option<SongLuaOverlayModelLayer>) {
    assert_eq!(a.is_some(), b.is_some());
    if let (Some(a), Some(b)) = (a, b) {
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        if let (Some((ak, af)), Some((bk, bf))) = (&a.additive, &b.additive) {
            assert_eq!(ak, bk);
            compare_frames(af, bf);
        }
        for (av, bv) in a.vertices.iter().zip(b.vertices.iter()) {
            assert_eq!(av.normal.map(f32::to_bits), bv.normal.map(f32::to_bits));
            assert_eq!(av.pos.map(f32::to_bits), bv.pos.map(f32::to_bits));
            assert_eq!(av.uv.map(f32::to_bits), bv.uv.map(f32::to_bits));
        }
    }
}

#[test]
fn additive_frame_arrays_preserve_uv_and_cumulative_delay_bits() {
    for count in [
        None,
        Some(0),
        Some(1),
        Some(2),
        Some(8),
        Some(129),
        Some(1024),
    ] {
        for indexed in [false, true] {
            for mixed in [false, true] {
                let texture = texture(count, indexed, mixed);
                compare_frames(
                    &baseline::model_additive_frames(&texture),
                    &model_additive_frames(&texture),
                );
                let owner = Arc::clone(&texture.source);
                let before = format!("{owner:?}");
                black_box(model_additive_frames(&texture));
                assert_eq!(before, format!("{owner:?}"));
            }
        }
    }
}

#[test]
fn lua_layers_preserve_complete_model_and_additive_metadata_and_rejections() {
    for count in [None, Some(0), Some(1), Some(8), Some(129)] {
        for mixed in [false, true] {
            let mut slot = test_model_slot();
            slot.model_additive = Some(Arc::new(texture(count, true, mixed)));
            for frame in [0, 1, 7, 99] {
                compare_layers(
                    &baseline::model_layer_from_slot_frame(&slot, frame),
                    &model_layer_from_slot_frame(&slot, frame),
                );
            }
            slot.model_additive = None;
            compare_layers(
                &baseline::model_layer_from_slot_frame(&slot, 0),
                &model_layer_from_slot_frame(&slot, 0),
            );
            slot.model = None;
            assert!(model_layer_from_slot_frame(&slot, 0).is_none());
            slot.model = test_model_slot().model;
            Arc::make_mut(slot.model.as_mut().unwrap()).vertices = Arc::from([]);
            assert!(model_layer_from_slot_frame(&slot, 0).is_none());
        }
    }
}

#[test]
fn additive_frame_arrays_allocate_only_their_final_shared_storage() {
    for count in [None, Some(1), Some(8), Some(129), Some(1024)] {
        let texture = texture(count, false, false);
        let frames = count.unwrap_or(1);
        perf::assert_churn_budget(1, (frames * 20 + 16).next_multiple_of(8), || {
            black_box(model_additive_frames(&texture));
        });
        perf::assert_reduced_churn(
            || {
                black_box(baseline::model_additive_frames(&texture));
            },
            || {
                black_box(model_additive_frames(&texture));
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
fn benchmark_lua_model_frames() {
    for (name, count, mixed, iterations) in [
        ("atlas", None, false, 32768),
        ("empty", Some(0), false, 32768),
        ("frames8", Some(8), false, 16384),
        ("mixed129", Some(129), true, 4096),
        ("frames1024", Some(1024), false, 1024),
    ] {
        let texture = texture(count, true, mixed);
        compare_frames(
            &baseline::model_additive_frames(&texture),
            &model_additive_frames(&texture),
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("shared_additive_{name}_{label}"),
                iterations,
                1,
                || {
                    black_box(if new {
                        model_additive_frames(black_box(&texture))
                    } else {
                        baseline::model_additive_frames(black_box(&texture))
                    });
                },
            )
        });
    }
    for (name, count, iterations) in [
        ("atlas", None, 32768),
        ("frames8", Some(8), 16384),
        ("frames1024", Some(1024), 1024),
    ] {
        let mut slot = test_model_slot();
        slot.model_additive = Some(Arc::new(texture(count, true, false)));
        compare_layers(
            &baseline::model_layer_from_slot_frame(&slot, 0),
            &model_layer_from_slot_frame(&slot, 0),
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("shared_lua_layer_{name}_{label}"),
                iterations,
                1,
                || {
                    black_box(if new {
                        model_layer_from_slot_frame(black_box(&slot), black_box(0))
                    } else {
                        baseline::model_layer_from_slot_frame(black_box(&slot), black_box(0))
                    });
                },
            )
        });
    }
}
