use deadlib_render::{
    BlendMode, DrawOp, MeshRun, MeshVertex, RenderFrame, SpriteInstanceRaw, SpriteRun,
    TexturedMeshGeometry, TexturedMeshInstanceRaw, TexturedMeshRun, TexturedMeshVertex,
    TexturedMeshVertices,
    frame_compare::{FrameMismatch, compare_render_frames},
};
use glam::Mat4;
use std::sync::Arc;

fn render_fixture() -> RenderFrame {
    let geometry: Arc<[TexturedMeshVertex]> = Arc::from([TexturedMeshVertex::default(); 3]);
    RenderFrame {
        clear_color: [0.1, 0.2, 0.3, 1.0],
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: vec![sprite_instance()],
        mesh_vertices: vec![MeshVertex::default(); 3],
        tmesh_instances: vec![TexturedMeshInstanceRaw::new(
            Mat4::IDENTITY,
            [1.0; 4],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        )],
        tmesh_geometries: vec![TexturedMeshGeometry {
            vertices: TexturedMeshVertices::Shared(geometry),
            cache_key: 41,
        }],
        ops: vec![
            DrawOp::Sprite(SpriteRun {
                instance_start: 0,
                instance_count: 1,
                blend: BlendMode::Alpha,
                texture_handle: 7,
                camera: 0,
            }),
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 3,
                blend: BlendMode::Add,
                camera: 0,
            }),
            DrawOp::TexturedMesh(TexturedMeshRun {
                geometry: 0,
                instance_start: 0,
                instance_count: 1,
                blend: BlendMode::Alpha,
                texture_handle: 8,
                camera: 0,
                depth_test: true,
            }),
        ],
    }
}

fn sprite_instance() -> SpriteInstanceRaw {
    SpriteInstanceRaw {
        center: [0.0, 0.0, 0.0, 1.0],
        size: [1.0, 1.0],
        rot_sin_cos: [0.0, 1.0],
        tint: [1.0; 4],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        local_offset: [0.0; 2],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }
}

#[test]
fn cloned_frame_matches() {
    let expected = render_fixture();
    let actual = expected.clone();
    assert_eq!(compare_render_frames(&expected, &actual), Ok(()));
}

#[test]
fn clear_color_difference_is_reported() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    actual.clear_color[1] = 0.25;
    assert_eq!(
        compare_render_frames(&expected, &actual),
        Err(FrameMismatch {
            section: "frame",
            index: 0,
            field: "clear_color",
        })
    );
}

#[test]
fn retained_geometry_identity_is_compared() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    let copied: Arc<[TexturedMeshVertex]> = Arc::from(actual.tmesh_geometries[0].vertices.as_ref());
    actual.tmesh_geometries[0].vertices = TexturedMeshVertices::Shared(copied);
    assert_eq!(
        compare_render_frames(&expected, &actual),
        Err(FrameMismatch {
            section: "tmesh_geometry",
            index: 0,
            field: "identity",
        })
    );
}

#[test]
fn draw_operation_state_is_compared() {
    let expected = render_fixture();
    let mut actual = expected.clone();
    let DrawOp::Sprite(run) = &mut actual.ops[0] else {
        panic!("fixture starts with a sprite operation");
    };
    run.blend = BlendMode::Add;
    assert_eq!(
        compare_render_frames(&expected, &actual),
        Err(FrameMismatch {
            section: "draw_op",
            index: 0,
            field: "value",
        })
    );
}
