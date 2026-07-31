use deadlib_render::{
    BlendMode, MeshVertex, MeshVertices, ObjectType, RenderList, RenderObject, SpriteInstanceRaw,
    TexturedMeshInstanceRaw, TexturedMeshVertex, TexturedMeshVertices, build_render_batches,
    draw_prep::{self, DrawOp, DrawScratch},
    frame_compare::{FrameMismatch, compare_draw_scratch, compare_render_lists},
};
use glam::Mat4;
use std::sync::Arc;

fn render_fixture() -> RenderList {
    let mesh: Arc<[MeshVertex]> = Arc::from([
        MeshVertex {
            pos: [-1.0, 0.0],
            color: [1.0, 0.0, 0.0, 1.0],
        },
        MeshVertex {
            pos: [1.0, 0.0],
            color: [0.0, 1.0, 0.0, 1.0],
        },
    ]);
    let shared_tmesh: Arc<[TexturedMeshVertex]> = Arc::from([tmesh_vertex(0.0)]);
    let mut render_list = RenderList {
        clear_color: [0.1, 0.2, 0.3, 1.0],
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: vec![sprite_instance()],
        objects: vec![
            RenderObject {
                object_type: ObjectType::Sprite(0),
                texture_handle: 11,
                blend: BlendMode::Alpha,
                z: 1,
                order: 0,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::Mesh {
                    transform: Mat4::from_translation(glam::vec3(2.0, 3.0, 0.0)),
                    tint: [0.5, 0.6, 0.7, 0.8],
                    vertices: MeshVertices::Shared(mesh),
                },
                texture_handle: 0,
                blend: BlendMode::Add,
                z: 1,
                order: 1,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::TexturedMesh {
                    instance: tmesh_instance(),
                    vertices: TexturedMeshVertices::Shared(shared_tmesh),
                    geom_cache_key: 0,
                    depth_test: true,
                },
                texture_handle: 12,
                blend: BlendMode::Alpha,
                z: 1,
                order: 2,
                camera: 0,
            },
            RenderObject {
                object_type: ObjectType::TexturedMesh {
                    instance: tmesh_instance(),
                    vertices: TexturedMeshVertices::Transient(vec![tmesh_vertex(1.0)]),
                    geom_cache_key: 0,
                    depth_test: false,
                },
                texture_handle: 13,
                blend: BlendMode::Alpha,
                z: 1,
                order: 3,
                camera: 0,
            },
        ],
        batches: Vec::new(),
    };
    build_render_batches(&render_list.objects, &mut render_list.batches);
    render_list
}

fn sprite_instance() -> SpriteInstanceRaw {
    SpriteInstanceRaw {
        center: [1.0, 2.0, 3.0, 1.0],
        size: [4.0, 5.0],
        rot_sin_cos: [0.0, 1.0],
        tint: [1.0, 0.5, 0.25, 1.0],
        uv_scale: [0.5, 0.5],
        uv_offset: [0.25, 0.25],
        local_offset: [1.0, -1.0],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }
}

fn tmesh_vertex(x: f32) -> TexturedMeshVertex {
    TexturedMeshVertex {
        pos: [x, 2.0, 3.0],
        uv: [0.25, 0.75],
        color: [1.0, 1.0, 1.0, 1.0],
        tex_matrix_scale: [1.0, 1.0],
    }
}

fn tmesh_instance() -> TexturedMeshInstanceRaw {
    TexturedMeshInstanceRaw::new(
        Mat4::from_scale(glam::vec3(2.0, 2.0, 1.0)),
        [1.0, 1.0, 1.0, 1.0],
        [1.0, 1.0],
        [0.0, 0.0],
        [0.0, 0.0],
        false,
    )
}

fn mismatch(section: &'static str, index: usize, field: &'static str) -> FrameMismatch {
    FrameMismatch {
        section,
        index,
        field,
    }
}

#[test]
fn cloned_frame_matches_before_and_after_draw_prep() {
    let expected = render_fixture();
    let actual = expected.clone();
    assert_eq!(compare_render_lists(&expected, &actual), Ok(()));

    let mut expected_scratch = DrawScratch::default();
    let mut actual_scratch = DrawScratch::default();
    draw_prep::prepare(&expected, &mut expected_scratch, |_, _| false);
    draw_prep::prepare(&actual, &mut actual_scratch, |_, _| false);
    assert_eq!(
        compare_draw_scratch(&expected_scratch, &actual_scratch),
        Ok(())
    );
}

#[test]
fn camera_comparison_is_bit_exact() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    actual.cameras[0].w_axis.x = -0.0;

    assert_eq!(
        compare_render_lists(&expected, &actual),
        Err(mismatch("camera", 0, "value"))
    );
}

#[test]
fn shared_geometry_identity_is_part_of_the_contract() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    let ObjectType::TexturedMesh { vertices, .. } = &mut actual.objects[2].object_type else {
        panic!("fixture object must be a textured mesh");
    };
    let copied: Arc<[TexturedMeshVertex]> = Arc::from(vertices.as_ref().to_vec());
    *vertices = TexturedMeshVertices::Shared(copied);

    assert_eq!(
        compare_render_lists(&expected, &actual),
        Err(mismatch("tmesh_geometry", 2, "identity"))
    );
}

#[test]
fn transient_geometry_bytes_are_compared() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    let ObjectType::TexturedMesh { vertices, .. } = &mut actual.objects[3].object_type else {
        panic!("fixture object must be a textured mesh");
    };
    let TexturedMeshVertices::Transient(vertices) = vertices else {
        panic!("fixture geometry must be transient");
    };
    vertices[0].pos[0] = 9.0;

    assert_eq!(
        compare_render_lists(&expected, &actual),
        Err(mismatch("tmesh_geometry", 3, "bytes"))
    );
}

#[test]
fn prepared_draw_operation_state_is_compared() {
    let render_list = render_fixture();
    let mut expected = DrawScratch::default();
    let mut actual = DrawScratch::default();
    draw_prep::prepare(&render_list, &mut expected, |_, _| false);
    draw_prep::prepare(&render_list, &mut actual, |_, _| false);
    let DrawOp::Sprite(run) = &mut actual.ops[0] else {
        panic!("first fixture draw operation must be a sprite");
    };
    run.blend = BlendMode::Add;

    assert_eq!(
        compare_draw_scratch(&expected, &actual),
        Err(mismatch("draw_op", 0, "value"))
    );
}
