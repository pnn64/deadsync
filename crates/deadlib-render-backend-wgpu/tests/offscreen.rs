//! Pixel regressions for ordered AFT passes through the public wgpu backend.
#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use deadlib_render_backend_wgpu as backend;
use deadlib_render_core::{
    BlendMode, DrawOp, MeshRun, MeshVertex, PresentModePolicy, RenderFrame, RenderTargetFrame,
    SamplerDesc, SpriteInstanceRaw, SpriteRun, TexturedMeshGeometry, TexturedMeshInstanceRaw,
    TexturedMeshRun, TexturedMeshVertex, TexturedMeshVertices, render_target_texture_handle,
};
use glam::Mat4;
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event_loop::{ActiveEventLoop, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

struct Textures(backend::Texture);

impl backend::TextureLookup for Textures {
    fn wgpu_texture(&self, handle: u64) -> Option<&backend::Texture> {
        (handle == 1).then_some(&self.0)
    }
}

fn quad(bounds: [f32; 4], key: u64) -> TexturedMeshGeometry {
    let [left, right, bottom, top] = bounds;
    let vertices = [
        [left, bottom],
        [right, bottom],
        [right, top],
        [left, bottom],
        [right, top],
        [left, top],
    ]
    .map(|[x, y]| TexturedMeshVertex {
        pos: [x, y, 0.5],
        uv: [0.5; 2],
        color: [1.0; 4],
        tex_matrix_scale: [1.0; 2],
    });
    TexturedMeshGeometry {
        vertices: TexturedMeshVertices::Shared(Arc::from(vertices)),
        cache_key: key,
    }
}

fn mesh_op(geometry: u32, instance: u32, camera: u8) -> DrawOp {
    DrawOp::TexturedMesh(TexturedMeshRun {
        geometry,
        instance_start: instance,
        instance_count: 1,
        blend: BlendMode::Alpha,
        texture_handle: 1,
        camera,
        depth_test: true,
    })
}

fn instance(tint: [f32; 4], transform: Mat4) -> TexturedMeshInstanceRaw {
    TexturedMeshInstanceRaw::new(transform, tint, [1.0; 2], [0.0; 2], [0.0; 2], false)
}

fn sprite() -> SpriteInstanceRaw {
    SpriteInstanceRaw {
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
    }
}

fn sample(handle: u64) -> DrawOp {
    DrawOp::Sprite(SpriteRun {
        instance_start: 0,
        instance_count: 1,
        blend: BlendMode::Alpha,
        texture_handle: handle,
        camera: 0,
    })
}

fn target(id: u64) -> RenderTargetFrame {
    RenderTargetFrame {
        texture_handle: render_target_texture_handle(id),
        width: 64,
        height: 64,
        alpha: true,
        depth: true,
        preserve: false,
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: Vec::new(),
        mesh_vertices: Vec::new(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: Vec::new(),
        ops: Vec::new(),
    }
}

fn scene() -> RenderFrame {
    let mut first = target(1);
    first.alpha = false;
    first.width = 96;
    first.tmesh_geometries = vec![
        quad([-1.0, 0.0, -1.0, 1.0], 0),
        quad([-0.9, -0.6, 0.6, 0.9], 7),
    ];
    first.tmesh_instances = vec![
        instance([1.0, 0.0, 0.0, 1.0], Mat4::IDENTITY),
        instance(
            [0.0, 0.0, 1.0, 1.0],
            Mat4::from_translation([0.0, 0.0, -0.1].into()),
        ),
    ];
    first.ops = vec![mesh_op(0, 0, 0), mesh_op(1, 1, 0)];
    // Same local geometry index, different transient vertices; cached geometry
    // follows it in the first pass and precedes it in the dependent pass.
    let mut second = target(3);
    second.height = 96;
    second
        .cameras
        .push(Mat4::from_translation([0.1, 0.0, 0.0].into()));
    second.sprite_instances.push(sprite());
    second.tmesh_geometries = vec![
        quad([-0.9, -0.6, 0.6, 0.9], 7),
        quad([-0.1, 0.9, -1.0, 1.0], 0),
    ];
    second.tmesh_instances = vec![
        instance([0.0, 1.0, 0.0, 1.0], Mat4::IDENTITY),
        instance(
            [0.0, 0.0, 1.0, 1.0],
            Mat4::from_translation([1.5, 0.0, -0.1].into()),
        ),
    ];
    second.mesh_vertices = quad([-0.2, 0.2, -0.2, 0.2], 0)
        .vertices
        .as_ref()
        .iter()
        .map(|v| MeshVertex {
            pos: [v.pos[0], v.pos[1]],
            color: [1.0, 1.0, 0.0, 1.0],
        })
        .collect();
    second.ops = vec![
        sample(first.texture_handle),
        mesh_op(1, 0, 1),
        mesh_op(0, 1, 255),
        mesh_op(1, 0, 1), // Far geometry must not overwrite the nearer cached blue quad.
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Alpha,
            camera: 0,
        }),
    ];
    let handle = second.texture_handle;
    RenderFrame {
        clear_color: [0.0, 1.0, 1.0, 1.0],
        render_targets: vec![first, target(2), second],
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: vec![sprite()],
        mesh_vertices: Vec::new(),
        tmesh_geometries: vec![quad([-0.2, 0.2, -0.9, -0.6], 0)],
        tmesh_instances: vec![instance([1.0, 0.0, 1.0, 1.0], Mat4::IDENTITY)],
        ops: vec![sample(handle), mesh_op(0, 0, 0)],
    }
}

fn capture(state: &mut backend::State, frame: &RenderFrame, textures: &Textures) -> RgbaImage {
    backend::request_screenshot(state);
    backend::draw(state, frame, textures, true).expect("draw ordered passes");
    backend::capture_frame(state).expect("read back rendered pixels")
}

fn pixel(image: &RgbaImage, x: f32, y: f32, expected: [u8; 4]) {
    let x = ((x + 1.0) * 0.5 * image.width() as f32) as u32;
    let y = ((1.0 - y) * 0.5 * image.height() as f32) as u32;
    assert_eq!(image.get_pixel(x, y).0, expected, "pixel at {x}, {y}");
}

fn rotated_field(angle: f32, offscreen: bool) -> (RenderFrame, Mat4) {
    // The engine and ITGmania use OpenGL clip depth. Rotating an eight-column
    // field puts opposite halves on either side of zero, both still visible.
    let camera =
        glam::camera::rh::proj::opengl::orthographic(-427.0, 427.0, -240.0, 240.0, -1000.0, 1000.0)
            * Mat4::from_rotation_y(angle.to_radians());
    let mut field = target(4);
    field.width = 128;
    field.height = 128;
    field.cameras = vec![camera];
    field
        .tmesh_geometries
        .push(quad([-32.0, 32.0, -32.0, 32.0], 0));
    for column in 0..8 {
        let x = (column as f32 - 3.5) * 100.0;
        field.sprite_instances.push(SpriteInstanceRaw {
            center: [x, 150.0, 0.0, 0.0],
            size: [64.0; 2],
            tint: [1.0, 0.0, 0.0, 1.0],
            ..sprite()
        });
        field.mesh_vertices.extend(
            quad([x - 32.0, x + 32.0, -32.0, 32.0], 0)
                .vertices
                .as_ref()
                .iter()
                .map(|v| MeshVertex {
                    pos: [v.pos[0], v.pos[1]],
                    color: [0.0, 1.0, 0.0, 1.0],
                }),
        );
        field.tmesh_instances.push(instance(
            [0.0, 0.0, 1.0, 1.0],
            Mat4::from_translation([x, -150.0, -0.5].into()),
        ));
    }
    field.ops = vec![
        DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 8,
            blend: BlendMode::Alpha,
            texture_handle: 1,
            camera: 0,
        }),
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 48,
            blend: BlendMode::Alpha,
            camera: 0,
        }),
        DrawOp::TexturedMesh(TexturedMeshRun {
            geometry: 0,
            instance_start: 0,
            instance_count: 8,
            blend: BlendMode::Alpha,
            texture_handle: 1,
            camera: 0,
            depth_test: true,
        }),
    ];
    let frame = if offscreen {
        RenderFrame {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: vec![Mat4::IDENTITY],
            sprite_instances: vec![sprite()],
            ops: vec![sample(field.texture_handle)],
            render_targets: vec![field],
            mesh_vertices: Vec::new(),
            tmesh_instances: Vec::new(),
            tmesh_geometries: Vec::new(),
        }
    } else {
        RenderFrame {
            clear_color: [0.0, 0.0, 0.0, 1.0],
            cameras: field.cameras,
            sprite_instances: field.sprite_instances,
            mesh_vertices: field.mesh_vertices,
            tmesh_instances: field.tmesh_instances,
            tmesh_geometries: field.tmesh_geometries,
            ops: field.ops,
            render_targets: Vec::new(),
        }
    };
    (frame, camera)
}

fn check_rotated_field(state: &mut backend::State, textures: &Textures, api: &str) {
    for offscreen in [false, true] {
        for angle in [-20.0, 20.0] {
            let (mut frame, camera) = rotated_field(angle, offscreen);
            if offscreen {
                // RGB vibration also moves the final texture sprite in Z.
                frame.sprite_instances[0].center[2] = angle.signum() * 0.01;
            }
            let image = capture(state, &frame, textures);
            eprintln!("{api}: rotated field angle={angle} offscreen={offscreen}");
            for column in 0..8 {
                let x = (column as f32 - 3.5) * 100.0;
                for (y, color) in [
                    (150.0, [255, 0, 0, 255]),
                    (0.0, [0, 255, 0, 255]),
                    (-150.0, [0, 0, 255, 255]),
                ] {
                    let clip = camera * glam::Vec4::new(x, y, 0.0, 1.0);
                    pixel(&image, clip.x / clip.w, clip.y / clip.w, color);
                }
            }
        }
    }
}

struct CaptureApp {
    ran: bool,
}

impl ApplicationHandler for CaptureApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(128, 128)),
                )
                .expect("create hidden GPU test window"),
        );
        let benchmark = std::env::var_os("DEADSYNC_OFFSCREEN_BENCH").is_some();
        for api in ["dx12", "vulkan"] {
            let mut state = match api {
                "dx12" => backend::init_dx12(
                    Arc::clone(&window),
                    false,
                    PresentModePolicy::Immediate,
                    !benchmark,
                ),
                _ => backend::init_vulkan(
                    Arc::clone(&window),
                    false,
                    PresentModePolicy::Immediate,
                    !benchmark,
                ),
            }
            .expect("initialize GPU backend");
            backend::set_default_projection(&mut state, Mat4::IDENTITY);
            let textures = Textures(
                backend::create_texture(
                    &mut state,
                    &RgbaImage::from_pixel(1, 1, Rgba([255; 4])),
                    SamplerDesc::default(),
                )
                .expect("white texture"),
            );
            let mut frame = scene();
            let image = capture(&mut state, &frame, &textures);
            pixel(&image, -0.5, -0.5, [255, 0, 0, 255]);
            pixel(&image, 0.5, -0.5, [0, 255, 0, 255]);
            pixel(&image, -0.75, 0.75, [0, 0, 255, 255]);
            pixel(&image, 0.75, 0.75, [0, 0, 255, 255]);
            pixel(&image, 0.0, 0.0, [255, 255, 0, 255]);
            pixel(&image, 0.0, -0.75, [255, 0, 255, 255]);
            assert_eq!(
                capture(&mut state, &frame, &textures),
                image,
                "{api}: warm frame"
            );
            if benchmark {
                for count in [1, 8, 32] {
                    let mut workload = scene();
                    let template = workload.render_targets[2].clone();
                    workload.render_targets.truncate(1);
                    for index in 1..count {
                        let mut target = template.clone();
                        target.texture_handle = render_target_texture_handle(index as u64 + 1);
                        target.ops[0] = sample(render_target_texture_handle(index as u64));
                        workload.render_targets.push(target);
                    }
                    workload.ops[0] = sample(render_target_texture_handle(count as u64));
                    let mut samples = Vec::with_capacity(240);
                    for index in 0..270 {
                        let stats = backend::draw(&mut state, &workload, &textures, true)
                            .expect("timed draw");
                        if index >= 30 {
                            samples.push(
                                stats.backend_prepare_us
                                    + stats.backend_upload_us
                                    + stats.backend_setup_us
                                    + stats.backend_record_us
                                    + stats.submit_us,
                            );
                        }
                    }
                    samples.sort_unstable();
                    println!(
                        "{api} targets={count}: CPU prepare/upload/encode/submit median={}us p95={}us",
                        samples[120], samples[227]
                    );
                }
                // The benchmark uses these target handles too. Restore the test image.
                assert_eq!(capture(&mut state, &frame, &textures), image);
            }
            frame.render_targets[2].preserve = true;
            frame.render_targets[2].ops.clear();
            assert_eq!(
                capture(&mut state, &frame, &textures),
                image,
                "{api}: preserved target"
            );
            frame.render_targets[2].preserve = false;
            let cleared = capture(&mut state, &frame, &textures);
            pixel(&cleared, -0.5, -0.5, [0, 255, 255, 255]);
            pixel(&cleared, 0.0, -0.75, [255, 0, 255, 255]);
            // Zero and single-target workloads use the same path as the graph.
            frame.render_targets.truncate(1);
            frame.ops[0] = sample(frame.render_targets[0].texture_handle);
            let single = capture(&mut state, &frame, &textures);
            pixel(&single, -0.5, -0.5, [255, 0, 0, 255]);
            pixel(&single, 0.5, -0.5, [0, 0, 0, 255]);
            frame.render_targets.clear();
            frame.ops.remove(0);
            let main = capture(&mut state, &frame, &textures);
            pixel(&main, -0.5, -0.5, [0, 255, 255, 255]);
            pixel(&main, 0.0, -0.75, [255, 0, 255, 255]);
            check_rotated_field(&mut state, &textures, api);
            backend::wait_for_idle(&mut state);
            eprintln!("{api}: offscreen pixel regression passed");
        }
        self.ran = true;
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _: &ActiveEventLoop,
        _: winit::window::WindowId,
        _: winit::event::WindowEvent,
    ) {
    }
}

#[test]
#[ignore = "requires Windows DX12 and Vulkan adapters with surface readback"]
fn ordered_offscreen_pixels() {
    let event_loop = EventLoop::builder()
        .with_any_thread(true)
        .build()
        .expect("event loop");
    let mut app = CaptureApp { ran: false };
    event_loop.run_app(&mut app).expect("GPU test event loop");
    assert!(app.ran);
}
