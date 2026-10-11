use super::*;
use deadlib_present::actors::{FlatTexturedMesh, MeshEnvironment, MeshTexture};
use deadlib_render_core::TexturedMeshVertex;
use std::{hint::black_box, sync::Arc};
#[path = "../../../tests/support/paired_bench.rs"]
mod paired_bench;

// Starting-main ownership adapter, updated for the current rendering types.
fn original_capture(draw: FlatDraw) -> Actor {
    match draw {
        FlatDraw::Sprite(sprite) => Actor::Sprite {
            align: [0.5, 0.5],
            offset: sprite.center,
            world_z: sprite.world_z,
            size: [SizeSpec::Px(sprite.size[0]), SizeSpec::Px(sprite.size[1])],
            source: sprite.source,
            tint: sprite.tint,
            glow: sprite.glow,
            z: sprite.z,
            cell: None,
            grid: None,
            uv_rect: Some(sprite.uv_rect),
            visible: true,
            flip_x: sprite.flip_x,
            flip_y: sprite.flip_y,
            cropleft: 0.0,
            cropright: 0.0,
            croptop: 0.0,
            cropbottom: 0.0,
            fadeleft: sprite.fade[0],
            faderight: sprite.fade[1],
            fadetop: sprite.fade[2],
            fadebottom: sprite.fade[3],
            blend: sprite.blend,
            mask_source: false,
            mask_dest: false,
            rot_x_deg: sprite.rot_x_deg,
            rot_y_deg: sprite.rot_y_deg,
            rot_z_deg: sprite.rot_z_deg,
            skew: [0.0, 0.0],
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: None,
            animate: false,
            state_delay: 0.0,
            scale: [1.0, 1.0],
            shadow_len: [0.0, 0.0],
            shadow_color: [0.0; 4],
            effect: EffectState::default(),
        },
        FlatDraw::TexturedMesh(mesh) => {
            let make_actor = |vertices| Actor::TexturedMesh {
                environment: mesh.environment.clone(),
                align: [0.0, 0.0],
                offset: mesh.offset,
                world_z: mesh.world_z,
                size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                local_transform: mesh.local_transform,
                texture: mesh.texture.clone(),
                tint: mesh.tint,
                glow: mesh.glow,
                vertices,
                geom_cache_key: mesh.geom_cache_key,
                uv_scale: mesh.uv_scale,
                uv_offset: mesh.uv_offset,
                uv_tex_shift: mesh.uv_tex_shift,
                depth_test: mesh.depth_test,
                clear_depth: mesh.clear_depth,
                clear_depth_after: mesh.clear_depth_after,
                cull_mode: deadlib_render_core::CullMode::None,
                visible: true,
                blend: mesh.blend,
                z: mesh.z,
            };
            match mesh.vertices {
                FlatMeshVertices::Shared(vertices) => make_actor(vertices),
                FlatMeshVertices::Reusable(vertices) => Actor::ReusableTexturedMesh {
                    environment: mesh.environment.clone(),
                    align: [0.0, 0.0],
                    offset: mesh.offset,
                    world_z: mesh.world_z,
                    size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
                    local_transform: mesh.local_transform,
                    texture: mesh.texture,
                    tint: mesh.tint,
                    glow: mesh.glow,
                    vertices,
                    geom_cache_key: mesh.geom_cache_key,
                    uv_scale: mesh.uv_scale,
                    uv_offset: mesh.uv_offset,
                    uv_tex_shift: mesh.uv_tex_shift,
                    depth_test: mesh.depth_test,
                    clear_depth: mesh.clear_depth,
                    clear_depth_after: mesh.clear_depth_after,
                    cull_mode: deadlib_render_core::CullMode::None,
                    visible: true,
                    blend: mesh.blend,
                    z: mesh.z,
                },
            }
        }
        FlatDraw::PreparedU32(text) => prepared_text_actor(
            text.align,
            text.offset,
            text.color,
            text.font,
            TextContent::PreparedU32 {
                text: text.text,
                slot: text.slot,
            },
            text.align_text,
            text.z,
            text.scale,
            text.blend,
            text.shadow_len,
            text.shadow_color,
        ),
        FlatDraw::PreparedInline(text) => prepared_text_actor(
            text.align,
            text.offset,
            text.color,
            text.font,
            TextContent::FrameInline {
                text: text.text,
                slot: text.slot,
            },
            text.align_text,
            text.z,
            text.scale,
            text.blend,
            text.shadow_len,
            text.shadow_color,
        ),
    }
}

fn mesh(reusable: bool) -> FlatTexturedMesh {
    FlatTexturedMesh {
        environment: Some(MeshEnvironment {
            sampler: Some(deadlib_render_core::MeshSampler {
                filter: deadlib_render_core::SamplerFilter::Nearest,
                wrap: deadlib_render_core::SamplerWrap::Repeat,
            }),
            camera: None,
            transform: Matrix4::from_rotation_y(0.5),
            additive_texture: Some(Arc::from("reflection")),
            additive_uv: [0.1, 0.2, 0.8, 0.9],
        }),
        offset: [12.0, 34.0],
        world_z: 5.0,
        local_transform: Matrix4::from_rotation_z(0.25),
        texture: MeshTexture::from(Arc::<str>::from("capture-hold")),
        tint: [0.8, 0.7, 0.6, 0.5],
        glow: [1.0, 1.0, 1.0, 0.25],
        vertices: if reusable {
            FlatMeshVertices::Reusable(Arc::new(vec![TexturedMeshVertex::default(); 6]))
        } else {
            FlatMeshVertices::Shared(Arc::from([TexturedMeshVertex::default(); 6]))
        },
        geom_cache_key: 123,
        uv_scale: [0.5; 2],
        uv_offset: [0.1; 2],
        uv_tex_shift: [0.2; 2],
        depth_test: true,
        clear_depth: true,
        clear_depth_after: true,
        cull_mode: deadlib_render_core::CullMode::Back,
        blend: BlendMode::Add,
        z: 140,
    }
}

#[test]
fn capture_moves_resources_and_preserves_original_actor_fields() {
    for reusable in [false, true] {
        let mesh = mesh(reusable);
        let expected = original_capture(FlatDraw::TexturedMesh(mesh.clone()));
        let actual = actor_from_flat_draw(FlatDraw::TexturedMesh(mesh));
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
    }
}

#[test]
#[ignore = "paired release throughput benchmark"]
fn benchmark_mesh_capture() {
    for reusable in [false, true] {
        let mesh = mesh(reusable);
        paired_bench::compare(
            if reusable {
                "reusable mesh capture"
            } else {
                "shared mesh capture"
            },
            200_000,
            |current| {
                let input = black_box(mesh.clone());
                black_box(if current {
                    actor_from_flat_draw(FlatDraw::TexturedMesh(input))
                } else {
                    original_capture(FlatDraw::TexturedMesh(input))
                });
            },
        );
    }
}
