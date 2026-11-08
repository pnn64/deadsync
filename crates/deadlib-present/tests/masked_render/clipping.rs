//! Clipping ownership, payload reuse, and polygon buffer regression tests.
use super::*;

fn replace_frame(builder: &mut FrameBuilder, source: &EditableDraw, passes: usize) {
    builder.clear();
    for _ in 0..64 {
        builder.push(source.clone());
    }
    for _ in 0..passes {
        for i in 0..builder.len() {
            let mut object = builder.take_object(i);
            object.order = i as u32;
            builder.replace_object(i, object);
        }
    }
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
