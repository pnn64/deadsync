//! Fixed-function sphere mapping and GL_ADD material stages, without skin assets.
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

fn frame(mode: f32) -> RenderFrame {
    let vertices = [
        [-0.8, -0.8, 0.0],
        [0.8, -0.8, 0.0],
        [0.8, 0.8, 0.0],
        [-0.8, -0.8, 0.0],
        [0.8, 0.8, 0.0],
        [-0.8, 0.8, 0.0],
    ]
    .map(|pos| TexturedMeshVertex {
        normal: [0.0, 0.0, 1.0, mode],
        pos,
        uv: [0.1, 0.2],
        color: [1.0; 4],
        ..Default::default()
    });
    let mut instance = TexturedMeshInstanceRaw::new(
        Mat4::IDENTITY,
        [1.0; 4],
        [1.0; 2],
        [0.0; 2],
        [0.0; 2],
        false,
    );
    instance.sphere_rows = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, -1000.0],
    ];
    instance.additive_texture = 2;
    RenderFrame {
        clear_color: [20.0 / 255.0, 40.0 / 255.0, 60.0 / 255.0, 1.0],
        render_targets: vec![],
        cameras: vec![],
        sprite_instances: vec![],
        mesh_vertices: vec![],
        tmesh_instances: vec![instance],
        tmesh_geometries: vec![TexturedMeshGeometry {
            cache_key: 0,
            vertices: TexturedMeshVertices::Shared(Arc::from(vertices)),
        }],
        ops: vec![DrawOp::TexturedMesh(TexturedMeshRun {
            additive_texture: 2,
            geometry: 0,
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: 1,
            camera: 0,
            depth_test: false,
            clear_depth: false,
        })],
    }
}

#[test]
#[ignore = "requires graphics devices and a window system"]
fn sphere_material_pixels() {
    let events = EventLoop::builder()
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
            events
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(64, 64)),
                )
                .expect("hidden window"),
        );
        let mut backend = create_backend(
            kind,
            window,
            Mat4::IDENTITY,
            false,
            PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("backend");
        let mut textures = TextureHandleMap::default();
        let gradient = RgbaImage::from_fn(128, 128, |x, y| {
            Rgba([(x * 2) as u8, (y * 2) as u8, 0, 255])
        });
        textures.insert(
            1,
            backend
                .create_texture(&gradient, SamplerDesc::default())
                .expect("gradient"),
        );
        textures.insert(
            2,
            backend
                .create_texture(
                    &RgbaImage::from_pixel(1, 1, Rgba([80, 40, 20, 128])),
                    SamplerDesc::default(),
                )
                .expect("reflection"),
        );
        let mut draw = frame(1.0);
        for (angle, expected) in [
            (0.0_f32, [127, 127, 0]),
            (45.0, [218, 127, 0]),
            (-45.0, [37, 127, 0]),
        ] {
            let mut eye = Mat4::from_rotation_y(angle.to_radians());
            eye.w_axis.z = -1000.0;
            draw.tmesh_instances[0].sphere_rows = [
                eye.row(0).to_array(),
                eye.row(1).to_array(),
                eye.row(2).to_array(),
            ];
            assert_center(&mut backend, &draw, &textures, kind, expected);
        }
        textures.insert(
            1,
            backend
                .create_texture(
                    &RgbaImage::from_pixel(1, 1, Rgba([40, 60, 80, 128])),
                    SamplerDesc::default(),
                )
                .expect("base"),
        );
        draw = frame(6.0);
        draw.tmesh_instances[0].tint = [0.5, 0.75, 1.0, 0.5];
        // GL_ADD: RGB=(base*tint)+reflection; alpha=base.a*tint.a*reflection.a.
        assert_center(&mut backend, &draw, &textures, kind, [30, 46, 65]);
        draw.tmesh_instances[0].texture_mask = 1.0;
        // Glow ignores reflection RGB and alpha, and uses only the base mask.
        assert_center(&mut backend, &draw, &textures, kind, [47, 78, 109]);

        // AFT/capture paths use both material stages too. Sampling the target
        // with a sprite also exercises transitions back to the sprite layout.
        textures.insert(
            1,
            backend
                .create_texture(&gradient, SamplerDesc::default())
                .expect("gradient"),
        );
        textures.insert(
            2,
            backend
                .create_texture(
                    &RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 255])),
                    SamplerDesc::default(),
                )
                .expect("reflection"),
        );
        draw = frame(7.0);
        let mut eye = Mat4::from_rotation_y(45.0_f32.to_radians());
        eye.w_axis.z = -1000.0;
        draw.tmesh_instances[0].sphere_rows = [
            eye.row(0).to_array(),
            eye.row(1).to_array(),
            eye.row(2).to_array(),
        ];
        let target = render_target_texture_handle(1);
        draw.render_targets.push(RenderTargetFrame {
            texture_handle: target,
            width: 64,
            height: 64,
            alpha: true,
            depth: true,
            preserve: false,
            cameras: vec![Mat4::IDENTITY],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            tmesh_instances: std::mem::take(&mut draw.tmesh_instances),
            tmesh_geometries: std::mem::take(&mut draw.tmesh_geometries),
            ops: std::mem::take(&mut draw.ops),
        });
        draw.sprite_instances.push(SpriteInstanceRaw {
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
        draw.ops.push(DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: target,
            camera: 0,
        }));
        assert_center(&mut backend, &draw, &textures, kind, [228, 147, 30]);
    }
}

fn assert_center(
    backend: &mut Backend,
    frame: &RenderFrame,
    textures: &TextureHandleMap<Texture>,
    kind: BackendType,
    expected: [u8; 3],
) {
    backend.request_screenshot();
    backend.draw(frame, textures, false).expect("draw material");
    let image = backend.capture_frame().expect("capture material");
    let actual = image.get_pixel(image.width() / 2, image.height() / 2).0;
    for i in 0..3 {
        assert!(
            actual[i].abs_diff(expected[i]) <= 3,
            "{kind}: {actual:?}, expected {expected:?}"
        );
    }
}
