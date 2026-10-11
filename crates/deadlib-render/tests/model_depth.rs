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
                normal: [0.0; 4],
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
                    sampler: None,
                    additive_texture: 0,
                    geometry: i,
                    instance_start: i,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: 1,
                    camera: 0,
                    depth_test: true,
                    // Notes 0 and 1 share depth; note 2 is isolated from them.
                    clear_depth: i == 2,
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
        BackendType::DirectX,
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
            sampler: None,
            additive_texture: 0,
            geometry: 1,
            instance_start: 3,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: 1,
            camera: 0,
            depth_test: true,
            clear_depth: true,
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
            viewport: [96, 96],
            float_color: false,
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
        assert_padded_capture(&mut backend, &textures, kind);
        assert_depth_capture(&mut backend, &textures, kind);
        assert_float_capture(&mut backend, &textures, kind);
        drop(textures);
        backend.cleanup();
    }
}

fn assert_depth_capture(
    backend: &mut Backend,
    textures: &TextureHandleMap<Texture>,
    kind: BackendType,
) {
    let handle = render_target_texture_handle(18);
    let model = model_frame();
    let mut frame = RenderFrame {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        cameras: vec![],
        sprite_instances: vec![SpriteInstanceRaw {
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
        }],
        mesh_vertices: vec![],
        tmesh_instances: vec![],
        tmesh_geometries: vec![],
        ops: vec![DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: handle,
            camera: 0,
        })],
        render_targets: vec![RenderTargetFrame {
            texture_handle: handle,
            width: 64,
            height: 64,
            viewport: [64, 64],
            float_color: false,
            alpha: false,
            depth: false,
            preserve: false,
            cameras: vec![],
            sprite_instances: vec![SpriteInstanceRaw {
                center: [-0.95, 0.95, 0.0, 1.0],
                size: [0.1; 2],
                rot_sin_cos: [0.0, 1.0],
                tint: [1.0; 4],
                uv_scale: [1.0; 2],
                uv_offset: [0.0; 2],
                local_offset: [0.0; 2],
                local_offset_rot_sin_cos: [0.0, 1.0],
                edge_fade: [0.0; 4],
                texture_mask: 0.0,
            }],
            mesh_vertices: [
                [0.9, 0.9],
                [1.0, 0.9],
                [1.0, 1.0],
                [0.9, 0.9],
                [1.0, 1.0],
                [0.9, 1.0],
            ]
            .map(|pos| MeshVertex {
                pos,
                color: [1.0, 1.0, 0.0, 1.0],
            })
            .to_vec(),
            tmesh_instances: model.tmesh_instances,
            tmesh_geometries: model.tmesh_geometries,
            ops: [
                DrawOp::Sprite(SpriteRun {
                    instance_start: 0,
                    instance_count: 1,
                    blend: BlendMode::Alpha,
                    texture_handle: 1,
                    camera: 0,
                }),
                DrawOp::Mesh(MeshRun {
                    vertex_start: 0,
                    vertex_count: 6,
                    blend: BlendMode::Alpha,
                    camera: 0,
                }),
            ]
            .into_iter()
            .chain(model.ops)
            .collect(),
        }],
    };
    // RageDisplay_OGL attaches depth only for bWithDepthBuffer. Without it,
    // the later rear red quad covers the nearer green quad by paint order.
    // The final blue draw clears depth and must still cover both in either mode.
    for float_color in [false, true] {
        frame.render_targets[0].float_color = float_color;
        for alpha in [false, true] {
            frame.render_targets[0].alpha = alpha;
            for depth in [true, false, true, false] {
                frame.render_targets[0].depth = depth;
                backend.request_screenshot();
                backend
                    .draw(&frame, textures, false)
                    .expect("render depth option");
                let image = backend.capture_frame().expect("depth option pixels");
                for (x, y, expected) in [
                    (0.32, 0.5, if depth { [0, 255, 0] } else { [255, 0, 0] }),
                    (0.5, 0.5, [0, 0, 255]),
                    (0.79, 0.5, [255, 0, 0]),
                    (0.025, 0.025, [255, 255, 255]),
                    (0.975, 0.025, [255, 255, 0]),
                ] {
                    let pixel = image
                        .get_pixel(
                            (image.width() as f32 * x) as u32,
                            (image.height() as f32 * y) as u32,
                        )
                        .0;
                    assert_eq!(
                        pixel[..3],
                        expected,
                        "{kind}: depth={depth}, float={float_color}, alpha={alpha}, sample={x},{y}"
                    );
                }
            }
        }
    }

    let mut check = |frame: &RenderFrame, expected: [u8; 3], label: &str| {
        backend.request_screenshot();
        backend
            .draw(frame, textures, false)
            .expect("render retained depth");
        let image = backend.capture_frame().expect("retained depth pixels");
        let pixel = image
            .get_pixel(
                (image.width() as f32 * 0.32) as u32,
                (image.height() as f32 * 0.4) as u32,
            )
            .0;
        assert_eq!(pixel[..3], expected, "{kind}: {label}");
    };
    let model = model_frame();
    for float_color in [false, true] {
        // A new preserved target is initialized, then keeps its depth across
        // captures. Clearing color alone must not reset that retained depth.
        let target = &mut frame.render_targets[0];
        target.texture_handle = render_target_texture_handle(19);
        target.float_color = float_color;
        target.depth = true;
        target.alpha = true;
        target.preserve = true;
        target.viewport = [64, 64];
        target.tmesh_instances.clone_from(&model.tmesh_instances);
        target.tmesh_geometries.clone_from(&model.tmesh_geometries);
        target.ops = vec![model.ops[0].clone()];
        target.mesh_vertices = [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ]
        .map(|pos| MeshVertex {
            pos,
            color: [0.0, 0.0, 0.0, 1.0],
        })
        .to_vec();
        let DrawOp::Sprite(run) = &mut frame.ops[0] else {
            unreachable!()
        };
        run.texture_handle = target.texture_handle;
        check(&frame, [0, 255, 0], "first preserved capture");
        frame.render_targets[0].ops = vec![model.ops[1].clone()];
        check(&frame, [0, 255, 0], "depth retained across captures");
        frame.render_targets[0].viewport = [32, 32];
        frame.render_targets[0].ops = vec![DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Alpha,
            camera: 0,
        })];
        check(&frame, [0, 0, 0], "color overwritten in reduced viewport");
        frame.render_targets[0].viewport = [64, 64];
        frame.render_targets[0].ops = vec![model.ops[1].clone()];
        check(&frame, [0, 0, 0], "depth retained through viewport changes");
        let DrawOp::TexturedMesh(run) = &mut frame.render_targets[0].ops[0] else {
            unreachable!()
        };
        run.clear_depth = true;
        check(
            &frame,
            [255, 0, 0],
            "explicit depth clear on preserved capture",
        );
        frame.render_targets[0].preserve = false;
        frame.render_targets[0].ops = vec![model.ops[1].clone()];
        check(&frame, [255, 0, 0], "nonpreserved depth clear");

        // RageDisplay_OGL uses DEPTH_COMPONENT16 for AFTs. These depths are
        // different in the main buffer, but round to the same capture value.
        let front = 1.0 - 2.0 * (20000.0 / 65535.0);
        let target = &mut frame.render_targets[0];
        target.tmesh_instances[0].model_col3[2] = front - 0.4;
        target.tmesh_instances[1].model_col3[2] = front - 0.000005;
        target.ops = model.ops[..2].to_vec();
        let main = RenderFrame {
            render_targets: vec![],
            sprite_instances: vec![],
            mesh_vertices: vec![],
            cameras: vec![],
            tmesh_geometries: target.tmesh_geometries.clone(),
            tmesh_instances: target.tmesh_instances.clone(),
            ops: target.ops.clone(),
            clear_color: [0.0, 0.0, 0.0, 1.0],
        };
        check(&main, [0, 255, 0], "distinct depths in main buffer");
        check(&frame, [255, 0, 0], "equal quantized capture depths");
        // Clip planes still apply when no depth attachment is present.
        frame.render_targets[0].depth = false;
        frame.render_targets[0].ops = vec![model.ops[1].clone()];
        for environment in [false, true] {
            frame.render_targets[0].tmesh_geometries[1].vertices =
                TexturedMeshVertices::Shared(Arc::from(
                    model.tmesh_geometries[1]
                        .vertices
                        .as_ref()
                        .iter()
                        .copied()
                        .map(|mut vertex| {
                            vertex.normal = if environment {
                                [0.0, 0.0, 1.0, 1.0]
                            } else {
                                [0.0; 4]
                            };
                            vertex
                        })
                        .collect::<Vec<_>>(),
                ));
            for z in [-2.0, 2.0, 0.0] {
                frame.render_targets[0].tmesh_instances[1].model_col3[2] = z;
                check(
                    &frame,
                    if z == 0.0 { [255, 0, 0] } else { [0, 0, 0] },
                    "depth-disabled clip planes",
                );
            }
        }
    }
}

fn assert_float_capture(
    backend: &mut Backend,
    textures: &TextureHandleMap<Texture>,
    kind: BackendType,
) {
    let handle = render_target_texture_handle(15);
    let mut frame = model_frame();
    frame.clear_color = [0.0, 0.0, 0.0, 1.0];
    frame.tmesh_instances = vec![TexturedMeshInstanceRaw::new(
        Mat4::IDENTITY,
        [0.25, 0.25, 0.25, 1.0],
        [1.0; 2],
        [0.0; 2],
        [0.0; 2],
        false,
    )];
    frame.tmesh_geometries = vec![quad(1.0, 0.0)];
    frame.sprite_instances = vec![SpriteInstanceRaw {
        center: [0.0; 4],
        size: [2.0; 2],
        rot_sin_cos: [0.0, 1.0],
        tint: [0.25, 0.25, 0.25, 1.0],
        uv_scale: [1.0; 2],
        uv_offset: [0.0; 2],
        local_offset: [0.0; 2],
        local_offset_rot_sin_cos: [0.0, 1.0],
        edge_fade: [0.0; 4],
        texture_mask: 0.0,
    }];
    frame.render_targets = vec![RenderTargetFrame {
        texture_handle: handle,
        width: 64,
        height: 64,
        viewport: [64, 64],
        float_color: true,
        alpha: false,
        depth: true,
        preserve: false,
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: vec![],
        mesh_vertices: [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ]
        .map(|pos| MeshVertex {
            pos,
            color: [1.0, 0.0, 0.0, 1.0],
        })
        .to_vec(),
        tmesh_instances: vec![],
        tmesh_geometries: vec![],
        ops: vec![
            DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 6,
                blend: BlendMode::Add,
                camera: 0
            });
            2
        ],
    }];
    // The float capture holds RGB=2 and uses native source-over alpha. Sampling it
    // at lower intensity must recover those values, rather than a clamped 1.
    // Toggle format on the same handle to verify cache identity, then redraw it.
    for alpha in [false, true] {
        frame.render_targets[0].alpha = alpha;
        frame.sprite_instances[0].tint[3] = if alpha { 0.5 } else { 1.0 };
        frame.tmesh_instances[0].tint[3] = if alpha { 0.5 } else { 1.0 };
        for float_color in [true, false, true] {
            frame.render_targets[0].float_color = float_color;
            for textured in [false, true] {
                frame.ops = vec![if textured {
                    DrawOp::TexturedMesh(TexturedMeshRun {
                        sampler: None,
                        additive_texture: 0,
                        geometry: 0,
                        instance_start: 0,
                        instance_count: 1,
                        blend: BlendMode::Add,
                        texture_handle: handle,
                        camera: 0,
                        depth_test: false,
                        clear_depth: false,
                    })
                } else {
                    DrawOp::Sprite(SpriteRun {
                        instance_start: 0,
                        instance_count: 1,
                        blend: BlendMode::Add,
                        texture_handle: handle,
                        camera: 0,
                    })
                }];
                backend.request_screenshot();
                backend
                    .draw(&frame, textures, false)
                    .expect("render float capture");
                let image = backend.capture_frame().expect("float capture pixels");
                let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
                let red = if float_color {
                    if alpha { 64 } else { 128 }
                } else if alpha {
                    32
                } else {
                    64
                };
                assert!(
                    pixel[0].abs_diff(red) <= 1 && pixel[1] == 0 && pixel[2] == 0,
                    "{kind}: float={float_color}, alpha={alpha}, textured={textured}, pixel={pixel:?}, expected red={red}"
                );
                if float_color && !alpha && !textured {
                    frame.sprite_instances[0].tint = [1.0, 1.0, 1.0, 0.5];
                    backend.request_screenshot();
                    backend
                        .draw(&frame, textures, false)
                        .expect("sample bright float into byte target");
                    let image = backend.capture_frame().expect("bright byte blend pixels");
                    let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
                    assert!(
                        pixel[0].abs_diff(128) <= 1 && pixel[1] == 0 && pixel[2] == 0,
                        "{kind}: normalized fragment clamp, pixel={pixel:?}"
                    );
                    frame.sprite_instances[0].tint = [0.25, 0.25, 0.25, 1.0];
                }
            }
        }
    }
    // RageDisplay_OGL uses ONE / ONE_MINUS_SRC_ALPHA for the alpha channel,
    // independently of normal/additive RGB factors. Two half-alpha layers
    // therefore retain 0.75 alpha, which must survive sampling the capture.
    frame.render_targets[0].float_color = true;
    frame.render_targets[0].alpha = true;
    for vertex in &mut frame.render_targets[0].mesh_vertices {
        vertex.color[3] = 0.5;
    }
    frame.sprite_instances[0].tint = [1.0; 4];
    frame.ops = vec![DrawOp::Sprite(SpriteRun {
        instance_start: 0,
        instance_count: 1,
        blend: BlendMode::Alpha,
        texture_handle: handle,
        camera: 0,
    })];
    for (blend, expected) in [(BlendMode::Alpha, 143_u8), (BlendMode::Add, 191)] {
        for op in &mut frame.render_targets[0].ops {
            let DrawOp::Mesh(run) = op else {
                unreachable!()
            };
            run.blend = blend;
        }
        backend.request_screenshot();
        backend
            .draw(&frame, textures, false)
            .expect("render translucent capture");
        let image = backend.capture_frame().expect("translucent capture pixels");
        let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
        assert!(
            pixel[0].abs_diff(expected) <= 1 && pixel[1] == 0 && pixel[2] == 0,
            "{kind}: captured alpha, blend={blend:?}, pixel={pixel:?}, expected red={expected}"
        );
    }
    // Float attachments also retain negative subtraction results. Subtracting
    // twice gives -0.75, then adding one restores 0.25. An 8-bit target clips
    // the intermediate negative result and incorrectly restores one instead.
    frame.render_targets[0].alpha = false;
    let red = frame.render_targets[0]
        .mesh_vertices
        .iter()
        .copied()
        .map(|mut vertex| {
            vertex.color[3] = 1.0;
            vertex
        })
        .collect::<Vec<_>>();
    frame.render_targets[0].mesh_vertices.extend(red);
    for (index, vertex) in frame.render_targets[0].mesh_vertices.iter_mut().enumerate() {
        vertex.color = if index < 6 {
            [0.8, 0.0, 0.0, 1.0]
        } else {
            [0.5, 0.0, 0.0, 0.5]
        };
    }
    frame.render_targets[0].ops = vec![
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Alpha,
            camera: 0,
        }),
        DrawOp::Mesh(MeshRun {
            vertex_start: 6,
            vertex_count: 6,
            blend: BlendMode::Multiply,
            camera: 0,
        }),
    ];
    for float_color in [true, false] {
        frame.render_targets[0].float_color = float_color;
        backend.request_screenshot();
        backend
            .draw(&frame, textures, false)
            .expect("render multiply capture");
        let image = backend.capture_frame().expect("multiply capture pixels");
        let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
        assert_eq!(
            pixel[..3],
            [102, 0, 0],
            "{kind}: multiply float={float_color}"
        );
    }
    for (index, vertex) in frame.render_targets[0].mesh_vertices.iter_mut().enumerate() {
        vertex.color = [1.0, 0.0, 0.0, if index < 6 { 0.5 } else { 1.0 }];
    }
    frame.render_targets[0].ops = vec![
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Subtract,
            camera: 0,
        }),
        DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Subtract,
            camera: 0,
        }),
        DrawOp::Mesh(MeshRun {
            vertex_start: 6,
            vertex_count: 6,
            blend: BlendMode::Add,
            camera: 0,
        }),
    ];
    for float_color in [true, false] {
        frame.render_targets[0].float_color = float_color;
        backend.request_screenshot();
        backend
            .draw(&frame, textures, false)
            .expect("render subtractive capture");
        let image = backend.capture_frame().expect("subtractive capture pixels");
        let pixel = image.get_pixel(image.width() / 2, image.height() / 2).0;
        let expected = if float_color { 64_u8 } else { 255 };
        assert!(
            pixel[0].abs_diff(expected) <= 1 && pixel[1] == 0 && pixel[2] == 0,
            "{kind}: negative color, float={float_color}, pixel={pixel:?}, expected red={expected}"
        );
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

fn assert_padded_capture(
    backend: &mut Backend,
    textures: &TextureHandleMap<Texture>,
    kind: BackendType,
) {
    // ITGmania allocates a power-of-two texture but renders only the requested
    // viewport. Sharkmode samples it with logical/backing UV ratios.
    let handle = render_target_texture_handle(12);
    let mut vertices = Vec::new();
    for (bounds, color) in [
        ([-3.0, 3.0, -3.0, 3.0], [1.0, 0.0, 0.0, 1.0]),
        ([0.5, 0.9, -0.5, 0.5], [0.0, 0.0, 1.0, 1.0]),
        ([-0.9, -0.5, 0.5, 0.9], [0.0, 1.0, 0.0, 1.0]),
    ] {
        let [left, right, bottom, top] = bounds;
        vertices.extend(
            [
                [left, bottom],
                [right, bottom],
                [right, top],
                [left, bottom],
                [right, top],
                [left, top],
            ]
            .map(|pos| MeshVertex { pos, color }),
        );
    }
    let mut frame = RenderFrame {
        clear_color: [0.0, 1.0, 1.0, 1.0],
        render_targets: vec![RenderTargetFrame {
            texture_handle: handle,
            width: 128,
            height: 64,
            viewport: [80, 48],
            float_color: false,
            alpha: true,
            depth: false,
            preserve: false,
            cameras: vec![Mat4::IDENTITY],
            sprite_instances: vec![],
            mesh_vertices: vertices,
            tmesh_instances: vec![],
            tmesh_geometries: vec![],
            ops: vec![DrawOp::Mesh(MeshRun {
                vertex_start: 0,
                vertex_count: 18,
                blend: BlendMode::Alpha,
                camera: 0,
            })],
        }],
        cameras: vec![Mat4::IDENTITY],
        sprite_instances: vec![SpriteInstanceRaw {
            center: [0.0; 4],
            size: [2.0; 2],
            rot_sin_cos: [0.0, 1.0],
            tint: [1.0; 4],
            uv_scale: [80.0 / 128.0, 48.0 / 64.0],
            uv_offset: [0.0; 2],
            local_offset: [0.0; 2],
            local_offset_rot_sin_cos: [0.0, 1.0],
            edge_fade: [0.0; 4],
            texture_mask: 0.0,
        }],
        mesh_vertices: vec![],
        tmesh_instances: vec![],
        tmesh_geometries: vec![],
        ops: vec![DrawOp::Sprite(SpriteRun {
            instance_start: 0,
            instance_count: 1,
            blend: BlendMode::Alpha,
            texture_handle: handle,
            camera: 0,
        })],
    };
    for alpha in [true, false] {
        frame.render_targets[0].alpha = alpha;
        frame.render_targets[0].preserve = false;
        frame.render_targets[0].ops = vec![DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 18,
            blend: BlendMode::Alpha,
            camera: 0,
        })];
        frame.render_targets[0].viewport = [80, 48];
        frame.sprite_instances[0].uv_scale = [80.0 / 128.0, 48.0 / 64.0];
        for _ in 0..2 {
            backend.request_screenshot();
            backend
                .draw(&frame, textures, false)
                .expect("padded capture");
            let image = backend.capture_frame().expect("padded capture pixels");
            for (x, y, expected) in [
                (0.85, 0.5, [0, 0, 255]),
                (0.15, 0.15, [0, 255, 0]),
                (0.15, 0.85, [255, 0, 0]),
            ] {
                let pixel = image
                    .get_pixel(
                        (image.width() as f32 * x) as u32,
                        (image.height() as f32 * y) as u32,
                    )
                    .0;
                assert_eq!(pixel[..3], expected, "{kind}: padded image at {x},{y}");
            }
        }
        frame.sprite_instances[0].uv_scale = [1.0; 2];
        frame.render_targets[0].preserve = true;
        frame.render_targets[0].ops.clear();
        // Changing only the viewport must keep the existing backing image.
        frame.render_targets[0].viewport = [64, 32];
        for preserve in [true, false] {
            frame.render_targets[0].preserve = preserve;
            backend.request_screenshot();
            backend
                .draw(&frame, textures, false)
                .expect("capture padding clear");
            let image = backend.capture_frame().expect("capture padding pixels");
            let empty = if alpha { [0, 255, 255] } else { [0, 0, 0] };
            for (x, y) in [(0.85, 0.5), (0.15, 0.9)] {
                let pixel = image
                    .get_pixel(
                        (image.width() as f32 * x) as u32,
                        (image.height() as f32 * y) as u32,
                    )
                    .0;
                assert_eq!(pixel[..3], empty, "{kind}: backing padding {x},{y}");
            }
            let pixel = image.get_pixel(image.width() / 4, image.height() / 2).0;
            assert_eq!(
                pixel[..3],
                if preserve { [255, 0, 0] } else { empty },
                "{kind}: preserve after viewport change"
            );
        }
    }
}
