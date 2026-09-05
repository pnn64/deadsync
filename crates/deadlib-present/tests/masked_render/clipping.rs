//! Clipping ownership, payload reuse, and polygon buffer benchmarks.
use super::*;

fn contained_scene() -> Vec<actors::Actor> {
    let mut actors = text_scene(8);
    for actor in &mut actors[..8] {
        if let actors::Actor::Sprite { size, offset, .. } = actor {
            *size = [SizeSpec::Px(200.0); 2];
            *offset = [50.0; 2];
        }
    }
    actors
}

fn polygon(points: &[[f32; 2]]) -> Vec<ClipVertex> {
    points
        .iter()
        .enumerate()
        .map(|(i, &pos)| ClipVertex {
            pos,
            uv: [i as f32 * 0.23, 1.0 - i as f32 * 0.17],
            color: [i as f32 * 0.2, 0.5, 0.75, 0.8],
        })
        .collect()
}

fn polygons() -> [Vec<ClipVertex>; 4] {
    [
        polygon(&[[-4.0, -4.0], [4.0, -4.0], [4.0, 4.0]]),
        polygon(&[[-5.0, -1.0], [1.0, -5.0], [5.0, 1.0], [-1.0, 5.0]]),
        polygon(&[[-1.0, -1.0], [1.0, -1.0], [0.0, 1.0]]),
        polygon(&[[20.0, 20.0], [24.0, 20.0], [24.0, 24.0]]),
    ]
}

fn replace_frame(builder: &mut FrameBuilder, source: &EditableDraw, passes: usize) {
    builder.clear();
    for _ in 0..64 {
        builder.push(source.clone());
    }
    for _ in 0..passes {
        for i in 0..builder.len() {
            let mut object = builder.take_object(i);
            object.order = i as u32;
            builder.replace_object(i, black_box(object));
        }
    }
    black_box(&builder.items);
    black_box(&builder.textured_meshes);
}

#[test]
#[ignore = "release CPU/allocation benchmark; run with --ignored --nocapture --test-threads=1"]
fn clipping_bench() {
    let containing = rect(-8.0, 8.0, -8.0, 8.0);
    let partial = rect(-3.0, 3.0, -3.0, 3.0);
    let vertices = quad().repeat(256);
    let mut cold = mesh(
        renderer::TexturedMeshVertices::Transient(vertices.clone()),
        Matrix4::IDENTITY,
    );
    let mut cold_pool = Vec::new();
    crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
    let kept = clip_object_to_world_masks(&mut cold, &mut [], &[containing; 8], &mut cold_pool);
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    assert!(kept);
    println!(
        "COLD contained_transient_8 allocs={} reallocs={} frees={} alloc_bytes={} freed_bytes={}",
        heap.allocs, heap.reallocs, heap.frees, heap.allocated, heap.freed
    );
    for (name, masks, transient) in [
        ("contained_transient_8", vec![containing; 8], true),
        ("contained_shared_8", vec![containing; 8], false),
        (
            "mixed_contained_8",
            vec![
                partial, containing, partial, containing, containing, partial, containing,
                containing,
            ],
            true,
        ),
        ("partial_masks_control", vec![partial; 2], true),
        (
            "rejected_masks_control",
            vec![rect(10.0, 12.0, 10.0, 12.0); 8],
            false,
        ),
    ] {
        let shared: Arc<[renderer::TexturedMeshVertex]> = Arc::from(vertices.clone());
        let mut pool = Vec::with_capacity(128);
        measure(name, vertices.len(), || {
            let source = if transient {
                let mut buffer = take_recycled_text_mesh_vertices(&mut pool);
                buffer.extend_from_slice(black_box(&vertices));
                renderer::TexturedMeshVertices::Transient(buffer)
            } else {
                renderer::TexturedMeshVertices::Shared(Arc::clone(&shared))
            };
            let mut object = mesh(source, Matrix4::IDENTITY);
            black_box(clip_object_to_world_masks(
                &mut object,
                &mut [],
                black_box(&masks),
                &mut pool,
            ));
            black_box(&object);
            recycle_transient_object_vertices(object.object_type, &mut pool);
        });
        report_pool(name, &pool);
    }
    let source = mesh(
        renderer::TexturedMeshVertices::Shared(Arc::from(quad())),
        Matrix4::IDENTITY,
    );
    for (name, passes) in [("replace_payload_64", 1), ("replace_nested_64", 8)] {
        let mut builder = FrameBuilder::default();
        measure(name, 64 * passes, || {
            replace_frame(&mut builder, &source, passes)
        });
        println!(
            "PAYLOAD_STORAGE {name} slots={} bytes={} item_bytes={}",
            builder.textured_meshes.len(),
            builder.textured_meshes.capacity() * std::mem::size_of::<Option<TexturedMeshPayload>>(),
            builder.items.capacity() * std::mem::size_of::<DrawItem>()
        );
    }
    for (name, p) in [
        "polygon_triangle_128",
        "polygon_quad_128",
        "polygon_inside_control",
        "polygon_reject_control",
    ]
    .into_iter()
    .zip(polygons())
    {
        measure(name, 128, || {
            for _ in 0..128 {
                black_box(clip_polygon_to_world_rect(
                    black_box(&p),
                    black_box(partial),
                ));
            }
        });
    }
    let fonts = fixture_fonts();
    let metrics = Metrics {
        left: 0.0,
        right: 100.0,
        top: 100.0,
        bottom: 0.0,
    };
    let mut rotated = vec![sprite_actor(0.0, true)];
    rotated.extend((0..64).map(|_| sprite_actor(23.0, false)));
    for (name, actors, units) in [
        ("compose_contained_text_32", contained_scene(), 32),
        ("compose_clipped_text_32", text_scene(0), 32),
        ("compose_rotated_clip_64", rotated, 64),
    ] {
        let mut scratch = ComposeScratch::default();
        let mut text = TextLayoutCache::default();
        measure(name, units, || {
            let mut frame = build_screen_cached_with_scratch_and_texture_context(
                black_box(&actors),
                [0.0; 4],
                &metrics,
                &fonts,
                0.25,
                &mut text,
                &mut scratch,
                &Textures,
            );
            black_box(&frame);
            scratch.recycle_frame(&mut frame);
        });
        report_pool(name, &scratch.recycled_text_mesh_vertices);
    }
}

#[test]
#[ignore = "writes explicit old/new output to DEADSYNC_RENDER_SNAPSHOT"]
fn clipping_snapshot() {
    let mut out = String::new();
    let masks = [
        rect(-8.0, 8.0, -8.0, 8.0),
        rect(-3.0, 3.0, -3.0, 3.0),
        rect(-4.0, 0.0, -4.0, 4.0),
        rect(0.0, 4.0, -4.0, 4.0),
        rect(10.0, 12.0, 10.0, 12.0),
        rect(0.0, 0.0, -1.0, 1.0),
        rect(f32::NAN, 8.0, -8.0, 8.0),
    ];
    let mut projective = Matrix4::from_rotation_z(0.23);
    projective.x_axis.w = 0.05;
    for transform in [
        Matrix4::IDENTITY,
        Matrix4::from_rotation_z(0.23),
        projective,
    ] {
        for count in [0, 1, 2, 3, 6, 12] {
            for a in masks {
                for b in masks {
                    for storage in 0..3 {
                        let mut vertices = quad().repeat(2);
                        vertices.truncate(count);
                        let vertices = match storage {
                            0 => renderer::TexturedMeshVertices::Transient(vertices),
                            1 => renderer::TexturedMeshVertices::Shared(Arc::from(vertices)),
                            _ => renderer::TexturedMeshVertices::Reusable(Arc::new(vertices)),
                        };
                        let mut object = mesh(vertices, transform);
                        let keep = clip_object_to_world_masks(
                            &mut object,
                            &mut [],
                            &[a, b, a],
                            &mut Vec::new(),
                        );
                        writeln!(out, "keep {keep}").expect("write fixture");
                        record_object(&mut out, &object, &[]);
                    }
                }
            }
        }
    }
    // Deterministic varied convex polygons cover winding, narrow intersections,
    // exact edges and all four clipping planes. Capture every output float bit.
    for sides in [3, 4] {
        for i in 0..256 {
            let points: Vec<_> = (0..sides)
                .map(|j| {
                    let angle = j as f32 * std::f32::consts::TAU / sides as f32 + i as f32 * 0.17;
                    let (s, c) = angle.sin_cos();
                    [c * (0.01 + (i % 17) as f32), s * (0.01 + (i % 13) as f32)]
                })
                .collect();
            for clip in masks {
                let p = clip_polygon_to_world_rect(&polygon(&points), clip);
                writeln!(out, "polygon {}", p.len()).expect("write fixture");
                for v in p {
                    writeln!(
                        out,
                        "{:?} {:?} {:?}",
                        v.pos.map(f32::to_bits),
                        v.uv.map(f32::to_bits),
                        v.color.map(f32::to_bits)
                    )
                    .expect("write fixture");
                }
            }
        }
    }
    let mut builder = FrameBuilder::default();
    let source = mesh(
        renderer::TexturedMeshVertices::Shared(Arc::from(quad())),
        Matrix4::IDENTITY,
    );
    replace_frame(&mut builder, &source, 8);
    for i in 0..builder.len() {
        record_object(&mut out, &builder.take_object(i), &[]);
    }
    let fonts = fixture_fonts();
    let mut scratch = ComposeScratch::default();
    let mut text = TextLayoutCache::default();
    for actors in [contained_scene(), text_scene(0), text_scene(8)] {
        let mut frame = build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.0; 4],
            &Metrics {
                left: 0.0,
                right: 100.0,
                top: 100.0,
                bottom: 0.0,
            },
            &fonts,
            0.25,
            &mut text,
            &mut scratch,
            &Textures,
        );
        reuse::record_frame(&mut out, &frame);
        scratch.recycle_frame(&mut frame);
    }
    std::fs::write(
        std::env::var_os("DEADSYNC_RENDER_SNAPSHOT").expect("snapshot path"),
        out,
    )
    .expect("write snapshot");
}

#[test]
fn contained_masks_keep_source_without_allocating() {
    let source = quad();
    let ptr = source.as_ptr();
    let mut object = mesh(
        renderer::TexturedMeshVertices::Transient(source),
        Matrix4::IDENTITY,
    );
    let mut pool = Vec::new();
    crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
    let keep = clip_object_to_world_masks(
        &mut object,
        &mut [],
        &[rect(-8.0, 8.0, -8.0, 8.0); 8],
        &mut pool,
    );
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    assert!(keep);
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
    let EditablePayload::TexturedMesh {
        vertices,
        depth_test,
        ..
    } = &object.object_type
    else {
        panic!("expected mesh")
    };
    assert_eq!(vertices.as_ptr(), ptr);
    assert_eq!(vertices.as_ref(), quad());
    assert!(*depth_test);
    assert!(pool.is_empty());
}

#[test]
fn nested_clips_reuse_payloads_after_sprite_conversion_and_rejection() {
    let mut builder = FrameBuilder::default();
    let mut sprites = vec![sprite(0.37), sprite(0.0), sprite(0.0)];
    sprites[1].center[0] = 100.0;
    for index in 0..3 {
        let mut object = sprite_draw();
        object.object_type = EditablePayload::Sprite(index);
        object.order = index;
        builder.push(object);
    }
    let mut pool = Vec::new();
    for radius in [4.0, 3.5, 3.0, 2.5] {
        clip_objects_range_to_world_rect(
            &mut builder,
            &mut sprites,
            0,
            0,
            rect(-radius, radius, -radius, radius),
            &mut pool,
        );
        assert_eq!(builder.len(), 2);
        assert_eq!(
            builder
                .items
                .iter()
                .map(|item| item.order)
                .collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(sprites.len(), 1);
        assert_eq!(builder.textured_meshes.len(), 1);
        assert_eq!(builder.textured_meshes.iter().flatten().count(), 1);
    }
    let object = builder.take_object(0);
    let EditablePayload::TexturedMesh { vertices, .. } = object.object_type else {
        panic!("expected rotated mesh")
    };
    assert!(
        vertices
            .iter()
            .all(|v| v.pos[0].abs() <= 2.500001 && v.pos[1].abs() <= 2.500001)
    );
}

#[test]
fn repeated_payload_replacement_preserves_metadata_without_growth() {
    let source = mesh(
        renderer::TexturedMeshVertices::Shared(Arc::from(quad())),
        Matrix4::IDENTITY,
    );
    let mut builder = FrameBuilder::default();
    replace_frame(&mut builder, &source, 0);
    crate::HEAP.with(|c| c.set(Some(crate::HeapStats::default())));
    for _ in 0..8 {
        for i in 0..64 {
            let mut object = builder.take_object(i);
            object.order = i as u32;
            builder.replace_object(i, object);
        }
    }
    let heap = crate::HEAP
        .with(|c| c.replace(None))
        .expect("heap measurement enabled");
    assert_eq!((heap.allocs, heap.reallocs, heap.frees), (0, 0, 0));
    assert_eq!(builder.textured_meshes.len(), 64);
    for i in 0..64 {
        let object = builder.take_object(i);
        assert_eq!(
            (object.order, object.z, object.texture_handle, object.blend),
            (i as u32, 3, 17, BlendMode::Add)
        );
        let EditablePayload::TexturedMesh {
            vertices,
            instance,
            depth_test,
            ..
        } = object.object_type
        else {
            panic!("expected mesh")
        };
        assert_eq!(vertices.as_ref(), quad());
        assert_eq!(instance.transform(), Matrix4::IDENTITY);
        assert!(depth_test);
    }
}

#[test]
fn colored_mesh_replacement_keeps_vertices_and_metadata() {
    let vertices = Arc::from([renderer::MeshVertex {
        pos: [1.0, 2.0],
        color: [0.25, 0.5, 0.75, 0.8],
    }]);
    let mut source = sprite_draw();
    source.object_type = EditablePayload::Mesh {
        transform: Matrix4::from_translation(Vector3::new(4.0, 5.0, 6.0)),
        tint: [0.1, 0.2, 0.3, 0.4],
        vertices: MeshVertices::Shared(Arc::clone(&vertices)),
    };
    let mut builder = FrameBuilder::default();
    builder.push(source);
    for order in 0..8 {
        let mut object = builder.take_object(0);
        object.order = order;
        object.camera = 2;
        builder.replace_object(0, object);
    }
    assert_eq!(builder.meshes.len(), 1);
    let object = builder.take_object(0);
    assert_eq!(
        (object.order, object.camera, object.z, object.blend),
        (7, 2, 3, BlendMode::Add)
    );
    let EditablePayload::Mesh {
        transform,
        tint,
        vertices: actual,
    } = object.object_type
    else {
        panic!("expected colored mesh")
    };
    assert_eq!(actual.as_ref(), vertices.as_ref());
    assert_eq!(actual.as_ptr(), vertices.as_ptr());
    assert_eq!(transform.w_axis, glam::Vec4::new(4.0, 5.0, 6.0, 1.0));
    assert_eq!(tint, [0.1, 0.2, 0.3, 0.4]);
}

#[test]
fn polygon_clipping_preserves_interpolated_uvs_and_colors() {
    let points = [[-4.0, -4.0], [4.0, -4.0], [4.0, 4.0], [-4.0, 4.0]];
    for reverse in [false, true] {
        let mut input: Vec<_> = points
            .map(|pos| {
                let uv = [(pos[0] + 4.0) / 8.0, (pos[1] + 4.0) / 8.0];
                ClipVertex {
                    pos,
                    uv,
                    color: [uv[0], uv[1], 0.25, 0.8],
                }
            })
            .to_vec();
        if reverse {
            input.reverse();
        }
        let clipped = clip_polygon_to_world_rect(&input, rect(-2.0, 2.0, -3.0, 3.0));
        assert_eq!(clipped.len(), 4);
        for vertex in clipped {
            assert_eq!(vertex.pos[0].abs(), 2.0);
            assert_eq!(vertex.pos[1].abs(), 3.0);
            assert_eq!(
                vertex.uv,
                [(vertex.pos[0] + 4.0) / 8.0, (vertex.pos[1] + 4.0) / 8.0]
            );
            assert_eq!(vertex.color, [vertex.uv[0], vertex.uv[1], 0.25, 0.8]);
        }
    }
}
