use super::*;
use renderer::frame_compare::compare_render_frames_semantic;
use std::hint::black_box;

#[path = "compose_perf_original.rs"]
mod original;
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

fn sprite(index: usize) -> renderer::SpriteInstanceRaw {
    renderer::SpriteInstanceRaw {
        center: [index as f32 % 48.0 - 24.0, 0.0, 3.0, 1.0],
        size: [16.0, 20.0],
        rot_sin_cos: [0.0, 1.0],
        tint: [0.25, 0.5, 0.75, 0.8],
        uv_scale: [-0.5, 0.25],
        uv_offset: [0.75, 0.1],
        local_offset: [2.0, -3.0],
        local_offset_rot_sin_cos: [0.6, 0.8],
        edge_fade: [0.1, 0.2, 0.3, 0.4],
        texture_mask: (index % 2) as f32,
    }
}

fn scene(count: usize, mixed: bool) -> (FrameBuilder, Vec<renderer::SpriteInstanceRaw>) {
    let mut builder = FrameBuilder::default();
    let mut sprites = Vec::new();
    let vertices: Arc<[renderer::TexturedMeshVertex]> = Arc::from([
        renderer::TexturedMeshVertex {
            pos: [-20.0, -20.0, 0.0],
            ..Default::default()
        },
        renderer::TexturedMeshVertex {
            pos: [20.0, -20.0, 0.0],
            ..Default::default()
        },
        renderer::TexturedMeshVertex {
            pos: [0.0, 20.0, 0.0],
            ..Default::default()
        },
    ]);
    for index in 0..count {
        let order = index as u32;
        if mixed && index % 3 == 1 {
            builder.push_mesh(
                9,
                order,
                2,
                BlendMode::Add,
                0,
                MeshPayload {
                    transform: Matrix4::from_translation(Vector3::new(2.0, 3.0, 0.0)),
                    tint: [0.7; 4],
                    vertices: MeshVertices::Shared(Arc::from([
                        renderer::MeshVertex {
                            pos: [0.0, 0.0],
                            color: [1.0; 4],
                        },
                        renderer::MeshVertex {
                            pos: [1.0, 0.0],
                            color: [1.0; 4],
                        },
                        renderer::MeshVertex {
                            pos: [0.0, 1.0],
                            color: [1.0; 4],
                        },
                    ])),
                },
            );
        } else if mixed && index % 3 == 2 {
            let mut instance = renderer::TexturedMeshInstanceRaw::new(
                Matrix4::IDENTITY,
                [0.8; 4],
                [0.5; 2],
                [0.2; 2],
                [0.1; 2],
                false,
            );
            instance.cull_mode = (index / 3 % 3) as f32;
            let sampler = match index / 3 % 3 {
                0 => None,
                1 => Some(renderer::MeshSampler {
                    filter: renderer::SamplerFilter::Nearest,
                    wrap: renderer::SamplerWrap::Clamp,
                }),
                _ => Some(renderer::MeshSampler {
                    filter: renderer::SamplerFilter::Linear,
                    wrap: renderer::SamplerWrap::Repeat,
                }),
            };
            builder.push_textured_mesh(
                11,
                order,
                -2,
                BlendMode::Alpha,
                0,
                TexturedMeshPayload {
                    instance,
                    vertices: renderer::TexturedMeshVertices::Shared(vertices.clone()),
                    geom_cache_key: index as u64 + 1,
                    depth_test: true,
                    clear_depth: index % 2 == 0,
                    clear_depth_after: true,
                    sampler,
                },
            );
        } else {
            builder.push_sprite(7, order, 4, BlendMode::Alpha, 0, sprites.len() as u32);
            sprites.push(sprite(index));
        }
    }
    (builder, sprites)
}

fn clone_builder(builder: &FrameBuilder) -> FrameBuilder {
    FrameBuilder {
        items: builder.items.clone(),
        meshes: builder.meshes.clone(),
        textured_meshes: builder.textured_meshes.clone(),
    }
}

fn finish(mut builder: FrameBuilder, sprites: Vec<renderer::SpriteInstanceRaw>) -> RenderFrame {
    let mut frame = RenderFrame {
        clear_color: [0.0; 4],
        render_targets: Vec::new(),
        cameras: vec![Matrix4::IDENTITY],
        sprite_instances: sprites,
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: Vec::new(),
        ops: Vec::new(),
    };
    finish_frame::<false>(
        &mut builder,
        &mut frame.mesh_vertices,
        &mut frame.tmesh_instances,
        &mut frame.tmesh_geometries,
        &mut frame.ops,
        &mut HashMap::default(),
    );
    frame
}

fn assert_frames_equal(expected: RenderFrame, actual: RenderFrame) {
    compare_render_frames_semantic(&expected, &actual).unwrap();
    assert_eq!(expected.ops, actual.ops);
    assert_eq!(
        format!("{:?}", expected.sprite_instances),
        format!("{:?}", actual.sprite_instances)
    );
    assert_eq!(
        format!("{:?}", expected.mesh_vertices),
        format!("{:?}", actual.mesh_vertices)
    );
    assert_eq!(
        format!("{:?}", expected.tmesh_instances),
        format!("{:?}", actual.tmesh_instances)
    );
    for (old, new) in expected
        .tmesh_geometries
        .iter()
        .zip(&actual.tmesh_geometries)
    {
        assert_eq!(old.cache_key, new.cache_key);
        assert_eq!(
            format!("{:?}", old.vertices.as_ref()),
            format!("{:?}", new.vertices.as_ref())
        );
    }
}

#[test]
fn retained_capture_preserves_mixed_payloads_prefixes_and_rejection() {
    for mixed in [false, true] {
        let (mut source, sprites) = scene(24, mixed);
        // Retained capture follows item order even when payload slots are reordered.
        source.items.swap(3, 6);
        for start in [0, 1, 3, 24, 25] {
            let sprite_start = source.items[..start.min(24)]
                .iter()
                .filter(|item| item.kind == DrawKind::Sprite)
                .count();
            let old = original::capture_retained_frame(&source, &sprites, start, sprite_start);
            let new = capture_retained_frame(&source, &sprites, start, sprite_start);
            assert_eq!(old.is_some(), new.is_some());
            if let (Some(old), Some(new)) = (old, new) {
                assert_eq!(old.builder.items, new.builder.items);
                assert_frames_equal(
                    finish(old.builder, old.sprite_instances),
                    finish(new.builder, new.sprite_instances),
                );
            }
        }
        assert!(capture_retained_frame(&source, &sprites, 0, sprites.len() + 1).is_none());
        if mixed {
            source.textured_meshes[0].as_mut().unwrap().vertices =
                renderer::TexturedMeshVertices::Transient(Vec::new());
            assert!(original::capture_retained_frame(&source, &sprites, 0, 0).is_none());
            assert!(capture_retained_frame(&source, &sprites, 0, 0).is_none());
            source.textured_meshes[0] = None;
            assert!(capture_retained_frame(&source, &sprites, 0, 0).is_none());
        }
    }
}

fn attributes(count: usize) -> Vec<actors::TextAttribute> {
    (0..count)
        .map(|index| actors::TextAttribute {
            start: index * 37 % 256,
            length: index * 19 % 300,
            color: [index as f32, 0.2, 0.3, 0.4],
            vertex_colors: (index % 3 == 0).then_some([[index as f32; 4]; 4]),
            glow: None,
        })
        .collect()
}

#[test]
fn lazy_attribute_expiry_preserves_precedence_skips_and_reused_scratch() {
    let mut old_scratch = original::TextAttrScratch::default();
    let mut new_scratch = TextAttrScratch::default();
    for count in [512, 1, 8, 64, 0, 256] {
        let mut attrs = attributes(count);
        if let Some(last) = attrs.last_mut() {
            last.start = 3;
            last.length = 500;
        }
        attrs.push(actors::TextAttribute {
            start: usize::MAX - 2,
            length: 10,
            color: [0.5; 4],
            vertex_colors: None,
            glow: None,
        });
        for stride in [1, 7, 129] {
            let mut old = original::TextAttrCursor::new(&attrs, &mut old_scratch).unwrap();
            let mut new = TextAttrCursor::new(&attrs, &mut new_scratch).unwrap();
            for at in (0..1024)
                .step_by(stride)
                .chain([usize::MAX - 2, usize::MAX - 1, usize::MAX])
            {
                let expected = attrs
                    .iter()
                    .rev()
                    .find(|attr| attr.start <= at && at < attr_end(attr))
                    .map_or([[1.0; 4]; 4], |attr| attr.colors());
                assert_eq!(old.colors_for(at), expected);
                assert_eq!(new.colors_for(at), expected, "count {count}, at {at}");
                assert_eq!(new.colors_for(at), expected, "repeated character");
            }
        }
    }
}

#[test]
fn direct_sprite_clipping_matches_original_with_meshes_and_rotated_promotions() {
    let clips = [
        WorldRect {
            left: -10.0,
            right: 10.0,
            bottom: -6.0,
            top: 8.0,
        },
        WorldRect {
            left: -100.0,
            right: 100.0,
            bottom: -100.0,
            top: 100.0,
        },
        WorldRect {
            left: 200.0,
            right: 220.0,
            bottom: 200.0,
            top: 220.0,
        },
        WorldRect {
            left: 10.0,
            right: -10.0,
            bottom: 0.0,
            top: 0.0,
        },
    ];
    for mixed in [false, true] {
        for rotation in [0.0_f32, 0.000_000_1, 0.7, std::f32::consts::PI] {
            for clip in clips {
                let (source, mut sprites) = scene(32, mixed);
                for (index, sprite) in sprites.iter_mut().enumerate() {
                    let (sine, cosine) = rotation.sin_cos();
                    sprite.rot_sin_cos = [sine, cosine];
                    if index % 7 == 0 {
                        sprite.size[0] = 0.0;
                    }
                    if index % 11 == 0 {
                        sprite.size[1] = -1.0;
                    }
                }
                let mut old = clone_builder(&source);
                let mut new = clone_builder(&source);
                let mut old_sprites = sprites.clone();
                let mut new_sprites = sprites;
                let mut old_pool = (0..64).map(|_| Vec::with_capacity(48)).collect();
                let mut new_pool = (0..64).map(|_| Vec::with_capacity(48)).collect();
                original::clip_objects_range_to_world_rect(
                    &mut old,
                    &mut old_sprites,
                    3,
                    if mixed { 1 } else { 3 },
                    clip,
                    &mut old_pool,
                );
                clip_objects_range_to_world_rect(
                    &mut new,
                    &mut new_sprites,
                    3,
                    if mixed { 1 } else { 3 },
                    clip,
                    &mut new_pool,
                );
                assert_eq!(old.items, new.items);
                assert_eq!(old_pool.len(), new_pool.len());
                assert_frames_equal(finish(old, old_sprites), finish(new, new_sprites));
            }
        }
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_retained_capture() {
    for mixed in [false, true] {
        let (builder, sprites) = scene(512, mixed);
        paired_bench::compare(
            if mixed {
                "retained capture (mixed)"
            } else {
                "retained capture (sprites)"
            },
            2_000,
            |current| {
                let (builder, sprites) = black_box((&builder, sprites.as_slice()));
                black_box(if current {
                    capture_retained_frame(builder, sprites, 0, 0)
                } else {
                    original::capture_retained_frame(builder, sprites, 0, 0)
                });
            },
        );
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_attribute_cursor() {
    for count in [8, 64, 512] {
        let attrs = attributes(count);
        let mut old_scratch = original::TextAttrScratch::default();
        let mut new_scratch = TextAttrScratch::default();
        paired_bench::compare(
            &format!("text attributes ({count}, 1024 characters)"),
            500,
            |current| {
                let attrs = black_box(attrs.as_slice());
                if current {
                    let mut cursor = TextAttrCursor::new(attrs, &mut new_scratch).unwrap();
                    for at in 0..1024 {
                        black_box(cursor.colors_for(black_box(at)));
                    }
                } else {
                    let mut cursor =
                        original::TextAttrCursor::new(attrs, &mut old_scratch).unwrap();
                    for at in 0..1024 {
                        black_box(cursor.colors_for(black_box(at)));
                    }
                }
            },
        );
        let heap = [&new_scratch.start_order, &new_scratch.active]
            .into_iter()
            .filter(|indices| indices.spilled())
            .map(|indices| indices.capacity() * std::mem::size_of::<usize>())
            .sum::<usize>();
        println!(
            "attribute scratch: original {} inline bytes + {} heap bytes, current {} inline bytes + {heap} heap bytes",
            std::mem::size_of_val(&old_scratch),
            old_scratch.heap_bytes(),
            std::mem::size_of_val(&new_scratch)
        );
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_sprite_clipping() {
    let (source, original_sprites) = scene(512, false);
    let mut builder = clone_builder(&source);
    let mut sprites = original_sprites.clone();
    let mut recycled = Vec::new();
    for extent in [10.0, 100.0] {
        let clip = WorldRect {
            left: -extent,
            right: extent,
            bottom: -extent,
            top: extent,
        };
        paired_bench::compare(
            &format!("single-mask sprites (512, extent {extent})"),
            2_000,
            |current| {
                builder.items.clone_from(&source.items);
                sprites.clone_from(&original_sprites);
                if current {
                    clip_objects_range_to_world_rect(
                        black_box(&mut builder),
                        black_box(&mut sprites),
                        0,
                        0,
                        black_box(clip),
                        &mut recycled,
                    );
                } else {
                    original::clip_objects_range_to_world_rect(
                        black_box(&mut builder),
                        black_box(&mut sprites),
                        0,
                        0,
                        black_box(clip),
                        &mut recycled,
                    );
                }
                black_box((&builder.items, &sprites));
            },
        );
    }
}
