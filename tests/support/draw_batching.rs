// Shared before/after pixel fixtures for native and software backends.
use deadlib_render_core::*;
use std::sync::Arc;

fn sprite(x: f32, tint: [f32; 4]) -> SpriteInstanceRaw {
    SpriteInstanceRaw {
        center: [x, 0.0, 0.0, 1.0],
        size: [1.0; 2],
        rot_sin_cos: [0.0, 1.0],
        tint,
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        local_offset: [0.0; 2],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }
}

pub fn fixture(kind: usize, blend: BlendMode, depth: bool, glow: bool) -> RenderFrame {
    let quad = [
        [-0.5, -0.5],
        [0.5, -0.5],
        [0.5, 0.5],
        [-0.5, -0.5],
        [0.5, 0.5],
        [-0.5, 0.5],
    ];
    let mut camera = ProjectionMatrix::IDENTITY.to_cols_array();
    camera[13] = 0.35;
    let mut f = RenderFrame {
        clear_color: [0.04, 0.07, 0.11, 1.0],
        render_targets: Vec::new(),
        cameras: vec![
            ProjectionMatrix::IDENTITY,
            ProjectionMatrix::from_cols_array(&camera),
        ],
        sprite_instances: Vec::new(),
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: vec![TexturedMeshGeometry {
            cache_key: 51,
            vertices: TexturedMeshVertices::Shared(Arc::from(quad.map(|p| TexturedMeshVertex {
                pos: [p[0], p[1], 0.0],
                uv: [p[0] + 0.5, p[1] + 0.5],
                color: [1.0; 4],
                ..Default::default()
            }))),
        }],
        ops: Vec::new(),
    };
    for i in 0..4u32 {
        let x = -0.3 + i as f32 * 0.17;
        let tint = [
            [0.7, 0.1, 0.3, 0.4],
            [0.1, 0.6, 0.2, 0.5],
            [0.2, 0.3, 0.8, 0.3],
            [0.4, 0.7, 0.2, 0.4],
        ][i as usize];
        let camera = u8::from(i == 3);
        match kind {
            0 => {
                let mut instance = sprite(x, tint);
                instance.texture_mask = f32::from(glow);
                f.sprite_instances.push(instance);
                f.ops.push(DrawOp::Sprite(SpriteRun {
                    instance_start: i,
                    instance_count: 1,
                    blend,
                    texture_handle: 7,
                    camera,
                }));
            }
            1 => {
                f.mesh_vertices.extend(quad.map(|p| MeshVertex {
                    pos: [p[0] + x, p[1]],
                    color: tint,
                }));
                f.ops.push(DrawOp::Mesh(MeshRun {
                    vertex_start: i * 6,
                    vertex_count: 6,
                    blend,
                    camera,
                }));
            }
            _ => {
                let mut transform = ProjectionMatrix::IDENTITY.to_cols_array();
                transform[12] = x;
                // Include overlapping equal-depth surfaces as well as distinct depths.
                transform[14] = [0.2, 0.6, 0.2, 0.4][i as usize];
                f.tmesh_instances.push(TexturedMeshInstanceRaw::new(
                    ProjectionMatrix::from_cols_array(&transform),
                    tint,
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    glow,
                ));
                f.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
                    geometry: 0,
                    instance_start: i,
                    instance_count: 1,
                    blend,
                    texture_handle: 7,
                    camera,
                    depth_test: depth,
                }));
            }
        }
    }
    f
}

pub fn coalesce(f: &RenderFrame) -> RenderFrame {
    let mut result = f.clone();
    // The first three operations share state; the last uses a different camera.
    match &mut result.ops[0] {
        DrawOp::Sprite(r) => r.instance_count = 3,
        DrawOp::Mesh(r) => r.vertex_count = 18,
        DrawOp::TexturedMesh(r) => r.instance_count = 3,
    }
    result.ops.drain(1..3);
    result
}

pub fn offscreen(mut f: RenderFrame) -> RenderFrame {
    let handle = render_target_texture_handle(9);
    let target = RenderTargetFrame {
        texture_handle: handle,
        width: 64,
        height: 64,
        alpha: true,
        depth: true,
        preserve: false,
        cameras: std::mem::take(&mut f.cameras),
        sprite_instances: std::mem::take(&mut f.sprite_instances),
        mesh_vertices: std::mem::take(&mut f.mesh_vertices),
        tmesh_instances: std::mem::take(&mut f.tmesh_instances),
        tmesh_geometries: std::mem::take(&mut f.tmesh_geometries),
        ops: std::mem::take(&mut f.ops),
    };
    f.render_targets.push(target);
    let mut sample = sprite(0.0, [1.0; 4]);
    sample.size = [2.0; 2];
    f.sprite_instances.push(sample);
    f.ops.push(DrawOp::Sprite(SpriteRun {
        instance_start: 0,
        instance_count: 1,
        blend: BlendMode::Alpha,
        texture_handle: handle,
        camera: 0,
    }));
    f
}
