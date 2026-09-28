//! Synthetic model overlap and depth-isolation regressions; no noteskin assets.
#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use deadlib_render::*;
use deadlib_render_core::ProjectionMatrix as Mat4;
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

fn quad(radius: f32, z: f32) -> TexturedMeshGeometry {
    TexturedMeshGeometry {
        cache_key: 0,
        vertices: TexturedMeshVertices::Shared(Arc::from(
            [
                [-radius, -radius, z],
                [radius, -radius, z],
                [radius, radius, z],
                [-radius, -radius, z],
                [radius, radius, z],
                [-radius, radius, z],
            ]
            .map(|pos| TexturedMeshVertex {
                pos,
                uv: [0.5; 2],
                color: [1.0; 4],
                tex_matrix_scale: [1.0; 2],
            }),
        )),
    }
}

fn model_frame() -> RenderFrame {
    RenderFrame {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        render_targets: vec![],
        cameras: vec![],
        sprite_instances: vec![],
        mesh_vertices: vec![],
        tmesh_geometries: vec![quad(0.5, 0.4), quad(0.8, 0.0), quad(0.15, -0.4)],
        tmesh_instances: [
            [0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
        ]
        .map(|tint| {
            TexturedMeshInstanceRaw::new(Mat4::IDENTITY, tint, [1.0; 2], [0.0; 2], [0.0; 2], false)
        })
        .to_vec(),
        ops: (0..3)
            .map(|i| {
                DrawOp::TexturedMesh(TexturedMeshRun {
                    geometry: i,
                    instance_start: i,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: 1,
                    camera: 0,
                    depth_test: true,
                    clear_depth: i != 1,
                    clear_depth_after: i != 0,
                })
            })
            .collect(),
    }
}

#[test]
#[ignore = "requires graphics devices and a window system"]
fn model_depth_isolation() {
    let event_loop = EventLoop::builder()
        .with_any_thread(true)
        .build()
        .expect("event loop");
    for kind in [
        BackendType::VulkanWgpu,
        BackendType::OpenGL,
        BackendType::Vulkan,
    ] {
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(96, 96)),
                )
                .expect("hidden window"),
        );
        // Like ITGmania, positive model Z points towards the camera.
        let projection = Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let mut backend = create_backend(
            kind,
            Arc::clone(&window),
            projection,
            false,
            PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("backend");
        let mut textures = TextureHandleMap::default();
        textures.insert(
            1,
            backend
                .create_texture(
                    &RgbaImage::from_pixel(1, 1, Rgba([255; 4])),
                    SamplerDesc::default(),
                )
                .expect("texture"),
        );
        let mut frame = model_frame();
        for angle in [0.0_f32, 15.0, 90.0] {
            for instance in &mut frame.tmesh_instances {
                *instance = TexturedMeshInstanceRaw::new(
                    Mat4::from_rotation_z(angle.to_radians()),
                    instance.tint,
                    [1.0; 2],
                    [0.0; 2],
                    [0.0; 2],
                    false,
                );
            }
            assert_pixels(&mut backend, &frame, &textures, kind);
        }
        let size = window
            .request_inner_size(PhysicalSize::new(128, 80))
            .unwrap_or_else(|| window.inner_size());
        backend.resize(size.width, size.height);
        assert_pixels(&mut backend, &frame, &textures, kind);

        // An unrelated 3D overlay must not inherit the last note's depth.
        let mut overlay = frame.clone();
        let mut instance = TexturedMeshInstanceRaw::new(
            Mat4::IDENTITY,
            [1.0, 1.0, 0.0, 1.0],
            [1.0; 2],
            [0.0; 2],
            [0.0; 2],
            false,
        );
        instance.model_col3[2] = -0.8;
        overlay.tmesh_instances.push(instance);
        overlay.ops.push(DrawOp::TexturedMesh(TexturedMeshRun {
            geometry: 1,
            instance_start: 3,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: 1,
            camera: 0,
            depth_test: true,
            clear_depth: false,
            clear_depth_after: false,
        }));
        backend.request_screenshot();
        backend
            .draw(&overlay, &textures, false)
            .expect("render later 3D overlay");
        let image = backend.capture_frame().expect("capture overlay");
        assert_eq!(
            image.get_pixel(image.width() / 2, image.height() / 2).0[..3],
            [255, 255, 0],
            "{kind}: depth leaked past note"
        );

        // Offscreen passes must obey the same depth contract.
        let target = render_target_texture_handle(1);
        frame.render_targets.push(RenderTargetFrame {
            texture_handle: target,
            width: 96,
            height: 96,
            alpha: true,
            depth: true,
            preserve: false,
            cameras: vec![projection],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_instances: std::mem::take(&mut frame.tmesh_instances),
            tmesh_geometries: std::mem::take(&mut frame.tmesh_geometries),
            ops: std::mem::take(&mut frame.ops),
        });
        // Use a sprite to sample the entire target, including its UV orientation.
        frame.sprite_instances.push(SpriteInstanceRaw {
            center: [0.0; 4],
            size: [2.0; 2],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [1.0; 2],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        });
        frame.ops.push(DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: target,
            camera: 0,
        }));
        assert_pixels(&mut backend, &frame, &textures, kind);
        drop(textures);
        backend.cleanup();
    }
}

fn assert_pixels(
    backend: &mut Backend,
    frame: &RenderFrame,
    textures: &TextureHandleMap<Texture>,
    kind: BackendType,
) {
    backend.request_screenshot();
    backend
        .draw(frame, textures, false)
        .expect("render synthetic model");
    let image = backend.capture_frame().expect("capture synthetic model");
    for (x, expected) in [(0.32, [0, 255, 0]), (0.5, [0, 0, 255]), (0.79, [255, 0, 0])] {
        let actual = image
            .get_pixel((image.width() as f32 * x) as u32, image.height() / 2)
            .0;
        assert_eq!(actual[..3], expected, "{kind}: sample at {x}");
    }
}
