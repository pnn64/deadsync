use super::*;
use crate::{asset_discovery_support::Tree, perf};
use deadsync_noteskin::model::ItgTextureFrame;
use std::hint::black_box;

mod baseline;

fn model_slot(count: usize, def: SpriteDefinition, mode: usize) -> SpriteSlot {
    let mut slot = test_model_slot();
    slot.def = def;
    slot.sphere_mapped = mode & 1 != 0;
    if mode & 2 != 0 {
        let mut additive = test_model_slot();
        additive.sphere_mapped = mode & 4 != 0;
        slot.model_additive = Some(Arc::new(additive));
    }
    slot.uv_velocity = if mode & 2 != 0 {
        [0.25, -0.5]
    } else {
        [0.0; 2]
    };
    slot.sprite_mesh = mode & 4 != 0;
    slot.model = Some(Arc::new(ModelMesh {
        vertices: (0..count)
            .map(|n| ModelVertex {
                normal: [n as f32 * 0.25, -0.0, -1.0],
                pos: [n as f32 - 0.75, -(n as f32), 0.5],
                uv: [n as f32 * 0.001, -0.25],
                tex_matrix_scale: [0.5, 2.0],
            })
            .collect(),
        bounds: [-1.0, 1.0, -2.0, 2.0, 0.0, 1.0],
    }));
    slot
}

fn compare_geometry(a: &[TexturedMeshVertex], b: &[TexturedMeshVertex]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.normal.map(f32::to_bits), b.normal.map(f32::to_bits));
        assert_eq!(a.pos.map(f32::to_bits), b.pos.map(f32::to_bits));
        assert_eq!(a.uv.map(f32::to_bits), b.uv.map(f32::to_bits));
        assert_eq!(a.color.map(f32::to_bits), b.color.map(f32::to_bits));
        assert_eq!(
            a.tex_matrix_scale.map(f32::to_bits),
            b.tex_matrix_scale.map(f32::to_bits)
        );
    }
}

#[test]
fn final_geometry_matches_parent_bits_for_transforms_and_texture_modes() {
    for count in [0, 1, 3, 6, 129, 4096] {
        for mode in 0..8 {
            for rotation_deg in [0, 90, -90, 180, 37] {
                for (mirror_h, mirror_v) in
                    [(false, false), (true, false), (false, true), (true, true)]
                {
                    let def = SpriteDefinition {
                        rotation_deg,
                        mirror_h,
                        mirror_v,
                        src: [-5, 7],
                        size: [31, 17],
                    };
                    let slot = model_slot(count, def, mode);
                    compare_geometry(
                        &baseline::build_model_geometry(&slot),
                        &build_model_geometry(&slot),
                    );
                }
            }
        }
    }
    let mut slot = model_slot(4, SpriteDefinition::default(), 7);
    let vertices = &mut Arc::make_mut(slot.model.as_mut().unwrap()).vertices;
    for (n, vertex) in Arc::make_mut(vertices).iter_mut().enumerate() {
        vertex.pos = [f32::from_bits(0x7fc00042), f32::INFINITY, f32::NEG_INFINITY];
        vertex.normal[n % 3] = -0.0;
        vertex.uv = [f32::from_bits(1), f32::MAX];
    }
    compare_geometry(
        &baseline::build_model_geometry(&slot),
        &build_model_geometry(&slot),
    );
}

#[test]
#[should_panic(expected = "model geometry requested for non-model noteskin slot")]
fn geometry_keeps_non_model_error() {
    let mut slot = test_model_slot();
    slot.model = None;
    black_box(build_model_geometry(&slot));
}

#[test]
fn geometry_uses_one_final_allocation_and_shared_empty_storage() {
    for count in [1, 6, 129, 4096] {
        let slot = model_slot(count, SpriteDefinition::default(), 3);
        perf::assert_churn_budget(
            1,
            (count * std::mem::size_of::<TexturedMeshVertex>() + 16)
                .next_multiple_of(std::mem::align_of::<usize>()),
            || {
                black_box(build_model_geometry(black_box(&slot)));
            },
        );
        perf::assert_reduced_churn(
            || {
                black_box(baseline::build_model_geometry(&slot));
            },
            || {
                black_box(build_model_geometry(&slot));
            },
        );
    }
    let slot = model_slot(0, SpriteDefinition::default(), 0);
    let first = build_model_geometry(&slot);
    assert!(Arc::ptr_eq(&first, &build_model_geometry(&slot)));
    perf::assert_no_churn(|| {
        black_box(build_model_geometry(&slot));
    });
}

fn animation(path: PathBuf, count: usize, mixed: bool) -> ItgTextureAnimation {
    ItgTextureAnimation {
        path,
        frames: (0..count)
            .map(|n| ItgTextureFrame {
                path: PathBuf::from("unused-for-metadata.png"),
                delay: if mixed {
                    [
                        0.0,
                        -0.0,
                        -1.0,
                        0.125,
                        0.3,
                        f32::NAN,
                        f32::INFINITY,
                        f32::NEG_INFINITY,
                    ][n % 8]
                } else {
                    0.125
                },
            })
            .collect(),
    }
}

fn compare_sources(a: &Arc<SpriteSource>, b: &Arc<SpriteSource>, frame_size: [i32; 2]) {
    // Includes dimensions, rate, timing/UV caches and invalid initial render handles.
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    let (
        SpriteSource::Animated {
            frame_durations: Some(ad),
            ..
        },
        SpriteSource::Animated {
            frame_durations: Some(bd),
            ..
        },
    ) = (a.as_ref(), b.as_ref())
    else {
        panic!("expected animated sources")
    };
    assert_eq!(
        ad.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        bd.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
    );
    let mut old = test_model_slot();
    old.source = Arc::clone(a);
    old.def.size = frame_size;
    let mut new = old.clone();
    new.source = Arc::clone(b);
    for time in [
        -100.0,
        -0.25,
        -0.0,
        0.0,
        0.125,
        0.25,
        0.3,
        1.0,
        10.0,
        1e6,
        f32::NAN,
        f32::INFINITY,
    ] {
        for beat in [0.0, 0.3, 99.0] {
            assert_eq!(old.frame_index(time, beat), new.frame_index(time, beat));
            assert_eq!(
                old.frame_index_from_phase(time),
                new.frame_index_from_phase(time)
            );
        }
    }
    for index in 0..a.frame_count() + 3 {
        for time in [-0.5, 0.0, 0.125, 4.0] {
            assert_eq!(
                old.uv_for_frame_at(index, time).map(f32::to_bits),
                new.uv_for_frame_at(index, time).map(f32::to_bits)
            );
            assert_eq!(
                old.uv_for_note_at(index, time, [0.25, -0.5])
                    .map(f32::to_bits),
                new.uv_for_note_at(index, time, [0.25, -0.5])
                    .map(f32::to_bits)
            );
        }
    }
}

fn grid(count: usize) -> (usize, usize) {
    let columns = (count as f32).sqrt().ceil() as usize;
    (columns, count.div_ceil(columns.max(1)))
}

#[test]
fn model_source_metadata_preserves_delays_timing_and_uv_bits() {
    for count in [1, 2, 3, 7, 16, 129, 1024] {
        for mixed in [false, true] {
            let animation = animation(PathBuf::from("metadata.ini"), count, mixed);
            let grid = grid(count);
            let frame_size = [7, 11];
            let dims = (grid.0 as u32 * 7, grid.1 as u32 * 11);
            compare_sources(
                &baseline::model_animation_source_data(
                    "fixture".into(),
                    dims,
                    frame_size,
                    grid,
                    &animation,
                ),
                &model_animation_source_data("fixture".into(), dims, frame_size, grid, &animation),
                frame_size,
            );
            perf::assert_reduced_churn(
                || {
                    black_box(baseline::model_animation_source_data(
                        "fixture".into(),
                        dims,
                        frame_size,
                        grid,
                        &animation,
                    ));
                },
                || {
                    black_box(model_animation_source_data(
                        "fixture".into(),
                        dims,
                        frame_size,
                        grid,
                        &animation,
                    ));
                },
            );
        }
    }
}

#[test]
fn registered_and_cold_model_animation_sources_and_errors_match_parent() {
    crate::noteskin::tests::init_asset_paths();
    for count in [1, 3, 17] {
        let tree = Tree::new();
        let frame = tree.path.join("frame.png");
        image::RgbaImage::from_fn(7, 11, |x, y| {
            image::Rgba([x as u8, y as u8, 37, if x % 2 == 0 { 0 } else { 255 }])
        })
        .save(&frame)
        .unwrap();
        let mut animation = animation(tree.path.join("animation.ini"), count, true);
        for f in &mut animation.frames {
            f.path = frame.clone();
        }
        // Cold new path registers the atlas; parent then observes the warm registry.
        let new = model_animation_source(&animation).unwrap();
        let old = baseline::model_animation_source(&animation).unwrap();
        compare_sources(&old, &new, [7, 11]);
        compare_sources(&old, &model_animation_source(&animation).unwrap(), [7, 11]);
        // A separate key exercises a cold parent path.
        animation.path = tree.path.join("parent-first.ini");
        let old = baseline::model_animation_source(&animation).unwrap();
        compare_sources(&old, &model_animation_source(&animation).unwrap(), [7, 11]);
        animation.frames.clear();
        assert_eq!(
            baseline::model_animation_source(&animation).unwrap_err(),
            model_animation_source(&animation).unwrap_err()
        );
        animation.frames.push(ItgTextureFrame {
            path: tree.path.join("missing.png"),
            delay: 0.5,
        });
        assert_eq!(
            baseline::model_animation_source(&animation).unwrap_err(),
            model_animation_source(&animation).unwrap_err()
        );
        let corrupt = tree.path.join("corrupt.png");
        std::fs::write(&corrupt, b"invalid image").unwrap();
        animation.frames[0].path = corrupt;
        assert_eq!(
            baseline::model_animation_source(&animation).unwrap_err(),
            model_animation_source(&animation).unwrap_err()
        );
        let oversized = tree.path.join("oversized.png");
        image::RgbaImage::new(8193, 1).save(&oversized).unwrap();
        animation.frames[0].path = oversized;
        assert_eq!(
            baseline::model_animation_source(&animation).unwrap_err(),
            model_animation_source(&animation).unwrap_err()
        );
    }
    // Later decode failure is still reported for a cold atlas.
    let tree = Tree::new();
    let frame = tree.path.join("frame.png");
    image::RgbaImage::new(2, 3).save(&frame).unwrap();
    let mut animation = animation(tree.path.join("bad.ini"), 2, false);
    animation.frames[0].path = frame;
    animation.frames[1].path = tree.path.join("missing.png");
    assert_eq!(
        baseline::model_animation_source(&animation).unwrap_err(),
        model_animation_source(&animation).unwrap_err()
    );
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

fn runtime_slot(count: usize) -> SpriteSlot {
    let mut slot = slot_from_plan(generated_animation_sprite_slot_plan(
        "runtime-benchmark".into(),
        (count.max(1) as u32, 16),
        [1, 16],
        count,
        AnimationRate::FramesPerSecond(1.0),
        false,
    ));
    slot.def.src = [3, 5];
    slot.def.mirror_h = true;
    baseline::apply_all_state_delays(&mut slot, 0.25, false);
    slot
}

fn durations(source: &Arc<SpriteSource>) -> &Arc<[f32]> {
    let SpriteSource::Animated {
        frame_durations: Some(durations),
        ..
    } = source.as_ref()
    else {
        panic!("expected delays")
    };
    durations
}

#[derive(Default)]
struct Owners {
    source: Option<Arc<SpriteSource>>,
    weak_source: Option<std::sync::Weak<SpriteSource>>,
    delays: Option<Arc<[f32]>>,
    weak_delays: Option<std::sync::Weak<[f32]>>,
}

fn owners(slot: &mut SpriteSlot, kind: usize) -> Owners {
    let mut owners = Owners::default();
    match kind {
        1 => owners.source = Some(Arc::clone(&slot.source)),
        2 => owners.weak_source = Some(Arc::downgrade(&slot.source)),
        3 => owners.delays = Some(Arc::clone(durations(&slot.source))),
        4 => owners.weak_delays = Some(Arc::downgrade(durations(&slot.source))),
        5 | 6 => {
            let SpriteSource::Animated {
                frame_durations, ..
            } = Arc::get_mut(&mut slot.source).unwrap()
            else {
                unreachable!()
            };
            *frame_durations = if kind == 5 {
                None
            } else {
                Some(Arc::from([0.3]))
            };
        }
        _ => {}
    }
    owners
}

#[test]
fn runtime_delay_updates_preserve_parent_sources_and_strong_or_weak_owners() {
    for count in [0, 1, 2, 16, 64, 65, 129, 1024] {
        for kind in 0..7 {
            for beat in [false, true] {
                for delay in [-1.0, -0.0, 0.0, 0.125, 0.3, f32::NAN, f32::INFINITY] {
                    let mut old = runtime_slot(count);
                    let mut new = runtime_slot(count);
                    let oldowners = owners(&mut old, kind);
                    let newowners = owners(&mut new, kind);
                    let oldsnapshot = format!("{:?}", oldowners.source);
                    let newsnapshot = format!("{:?}", newowners.source);
                    let newptr = match new.source.as_ref() {
                        SpriteSource::Animated {
                            frame_durations: Some(d),
                            ..
                        } => Some(d.as_ptr()),
                        _ => None,
                    };
                    baseline::apply_all_state_delays(&mut old, delay, beat);
                    apply_all_state_delays(&mut new, delay, beat);
                    compare_sources(&old.source, &new.source, [1, 16]);
                    assert_eq!(oldsnapshot, format!("{:?}", oldowners.source));
                    assert_eq!(newsnapshot, format!("{:?}", newowners.source));
                    assert_eq!(
                        format!("{:?}", oldowners.delays),
                        format!("{:?}", newowners.delays)
                    );
                    for (a, b) in [
                        (
                            oldowners
                                .weak_source
                                .as_ref()
                                .and_then(std::sync::Weak::upgrade)
                                .is_some(),
                            newowners
                                .weak_source
                                .as_ref()
                                .and_then(std::sync::Weak::upgrade)
                                .is_some(),
                        ),
                        (
                            oldowners
                                .weak_delays
                                .as_ref()
                                .and_then(std::sync::Weak::upgrade)
                                .is_some(),
                            newowners
                                .weak_delays
                                .as_ref()
                                .and_then(std::sync::Weak::upgrade)
                                .is_some(),
                        ),
                    ] {
                        assert_eq!(a, b);
                    }
                    if kind == 0 {
                        assert_eq!(newptr, Some(durations(&new.source).as_ptr()));
                    }
                }
            }
        }
    }
    let mut atlas = slot_from_plan(deadsync_noteskin::atlas_sprite_slot_plan(
        "atlas".into(),
        (8, 8),
        (8, 8),
        false,
    ));
    let source = Arc::clone(&atlas.source);
    perf::assert_no_churn(|| apply_all_state_delays(&mut atlas, 0.125, true));
    assert!(Arc::ptr_eq(&source, &atlas.source));
}

#[test]
fn runtime_delay_updates_reuse_unique_storage_and_reduce_large_shared_churn() {
    for count in [1, 16, 64, 65, 129, 1024] {
        let mut old = runtime_slot(count);
        let mut new = runtime_slot(count);
        perf::assert_no_churn(|| apply_all_state_delays(&mut new, 0.125, true));
        perf::assert_reduced_churn(
            || baseline::apply_all_state_delays(&mut old, 0.3, false),
            || apply_all_state_delays(&mut new, 0.3, false),
        );
        compare_sources(&old.source, &new.source, [1, 16]);
        let template = runtime_slot(count);
        let work = |old: bool| {
            let mut slot = template.clone();
            if old {
                baseline::apply_all_state_delays(&mut slot, 0.125, false);
            } else {
                apply_all_state_delays(&mut slot, 0.125, false);
            }
            black_box(slot);
        };
        if count > 64 {
            perf::assert_reduced_churn(|| work(true), || work(false));
        }
        // Fallback owns one delay array and one replacement source, at every size.
        perf::assert_churn_budget(
            2,
            (count * 4 + 16).next_multiple_of(8) + std::mem::size_of::<SpriteSource>() + 16,
            || work(false),
        );
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_model_source_preparation() {
    crate::noteskin::tests::init_asset_paths();
    for (name, count, mode, iterations) in [
        ("empty", 0, 0, 1024),
        ("triangle", 3, 0, 1024),
        ("mesh128", 128, 3, 512),
        ("mesh4096", 4096, 7, 64),
        ("mesh32768", 32768, 7, 16),
    ] {
        let slot = model_slot(
            count,
            SpriteDefinition {
                mirror_h: true,
                ..SpriteDefinition::default()
            },
            mode,
        );
        compare_geometry(
            &baseline::build_model_geometry(&slot),
            &build_model_geometry(&slot),
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("source_geometry_{name}_{label}"),
                iterations,
                1,
                || {
                    black_box(if new {
                        build_model_geometry(black_box(&slot))
                    } else {
                        baseline::build_model_geometry(black_box(&slot))
                    });
                },
            )
        });
    }
    for (name, count, mixed) in [
        ("one", 1, false),
        ("uniform16", 16, false),
        ("mixed129", 129, true),
        ("uniform1024", 1024, false),
    ] {
        let animation = animation(PathBuf::from("metadata.ini"), count, mixed);
        let grid = grid(count);
        let dims = (grid.0 as u32 * 16, grid.1 as u32 * 16);
        compare_sources(
            &baseline::model_animation_source_data(
                "benchmark".into(),
                dims,
                [16, 16],
                grid,
                &animation,
            ),
            &model_animation_source_data("benchmark".into(), dims, [16, 16], grid, &animation),
            [16, 16],
        );
        pairs(|label, new| {
            perf::measure_sampled(&format!("source_metadata_{name}_{label}"), 512, 1, || {
                let key = black_box("benchmark").to_owned();
                black_box(if new {
                    model_animation_source_data(
                        key,
                        black_box(dims),
                        black_box([16, 16]),
                        black_box(grid),
                        black_box(&animation),
                    )
                } else {
                    baseline::model_animation_source_data(
                        key,
                        black_box(dims),
                        black_box([16, 16]),
                        black_box(grid),
                        black_box(&animation),
                    )
                });
            })
        });
    }
    for count in [16, 1024] {
        let tree = Tree::new();
        let frame = tree.path.join("frame.png");
        image::RgbaImage::new(2, 3).save(&frame).unwrap();
        let mut animation = animation(tree.path.join("animation.ini"), count, false);
        for f in &mut animation.frames {
            f.path = frame.clone();
        }
        let source = model_animation_source(&animation).unwrap();
        compare_sources(
            &baseline::model_animation_source(&animation).unwrap(),
            &source,
            [2, 3],
        );
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("source_registered_frames{count}_{label}"),
                128,
                1,
                || {
                    black_box(if new {
                        model_animation_source(black_box(&animation)).unwrap()
                    } else {
                        baseline::model_animation_source(black_box(&animation)).unwrap()
                    });
                },
            )
        });
    }
    for count in [16, 1024] {
        pairs(|label, new| {
            let mut slot = runtime_slot(count);
            perf::measure_sampled(
                &format!("source_runtime_unique{count}_{label}"),
                512,
                1,
                || {
                    if new {
                        apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    } else {
                        baseline::apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    }
                    black_box(&slot);
                },
            );
        });
        let template = runtime_slot(count);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("source_runtime_shared{count}_{label}"),
                512,
                1,
                || {
                    let mut slot = black_box(&template).clone();
                    if new {
                        apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    } else {
                        baseline::apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    }
                    black_box(slot);
                },
            )
        });
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("source_runtime_owning{count}_{label}"),
                512,
                1,
                || {
                    let mut slot = runtime_slot(black_box(count));
                    if new {
                        apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    } else {
                        baseline::apply_all_state_delays(
                            black_box(&mut slot),
                            black_box(0.125),
                            black_box(true),
                        );
                    }
                    black_box(slot);
                },
            )
        });
    }
}
