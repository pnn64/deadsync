use super::*;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
};

#[allow(dead_code)]
#[path = "../../../tests/support/draw_batching.rs"]
mod fixtures;

struct TestTextures([Texture; 2]);

impl TextureLookup for TestTextures {
    fn wgpu_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        handle
            .checked_sub(7)
            .and_then(|index| self.0.get(index as usize))
    }
}

fn camera(op: &mut DrawOp) -> &mut u8 {
    match op {
        DrawOp::Sprite(run) => &mut run.camera,
        DrawOp::Mesh(run) => &mut run.camera,
        DrawOp::TexturedMesh(run) => &mut run.camera,
    }
}

fn interleaved_frame(blend: BlendMode, depth: bool, glow: bool, cached: bool) -> RenderFrame {
    let sprites = fixtures::fixture(0, blend, false, glow);
    let meshes = fixtures::fixture(1, blend, false, false);
    let mut frame = fixtures::fixture(2, blend, depth, glow);
    frame.sprite_instances = sprites.sprite_instances;
    frame.mesh_vertices = meshes.mesh_vertices;
    if !cached {
        frame.tmesh_geometries[0].cache_key = 0;
    }
    // A nonidentity projection makes a lost or stale upload visible.
    frame.cameras[0] = Matrix4::from_translation([0.15, -0.2, 0.0].into());
    let DrawOp::Sprite(mut yuv) = sprites.ops[0] else {
        unreachable!()
    };
    yuv.texture_handle = 8;
    let kinds = [
        sprites.ops[0],
        DrawOp::Sprite(yuv),
        frame.ops[0],
        meshes.ops[0],
    ];
    frame.ops.clear();
    // Every ordered pipeline pair uses the same camera across the transition.
    // Consecutive pairs also exercise camera changes and fallback selection.
    for (first, first_op) in kinds.iter().enumerate() {
        for (second, second_op) in kinds.iter().enumerate() {
            let selected = [0, 1, u8::MAX][(first + second) % 3];
            for mut op in [*first_op, *second_op] {
                *camera(&mut op) = selected;
                frame.ops.push(op);
            }
        }
    }
    frame
}

fn force_camera_uploads(frame: &RenderFrame, fallback: Matrix4) -> RenderFrame {
    let mut reference = frame.clone();
    reference.cameras.clear();
    for op in &mut reference.ops {
        let matrix = frame
            .cameras
            .get(*camera(op) as usize)
            .copied()
            .unwrap_or(fallback);
        // Distinct indices force an upload before every draw, even when the
        // matrices are identical. Rendering must match camera binding reuse.
        *camera(op) = reference.cameras.len() as u8;
        reference.cameras.push(matrix);
    }
    reference
}

fn capture(state: &mut State, frame: &RenderFrame, textures: &TestTextures) -> RgbaImage {
    request_screenshot(state);
    draw(state, frame, textures, false).expect("draw");
    capture_frame(state).expect("capture")
}

fn check_buffer_uploads(state: &State) {
    let destination = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("concatenated upload destination"),
        size: 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = state.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("concatenated upload readback"),
        size: 64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let cases: &[&[&[u8]]] = &[
        &[],
        &[&[], &[]],
        &[&[], &[1; 8], &[]],
        &[&[2; 4], &[], &[3; 12], &[4; 8], &[]],
        &[&[5; 32], &[6; 32]],
        &[&[7; 4], &[8; 4]],
    ];
    for chunks in cases {
        let bytes = chunks.concat();
        let mut expected = [0xa5; 64];
        expected[..bytes.len()].copy_from_slice(&bytes);
        state.queue.write_buffer(&destination, 0, &[0xa5; 64]);
        upload_buffer_slices(
            &state.queue,
            &destination,
            bytes.len(),
            chunks.iter().copied(),
        );
        let mut encoder = state.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(&destination, 0, &readback, 0, 64);
        state.queue.submit([encoder.finish()]);
        let (tx, rx) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            tx.send(result).unwrap()
        });
        state
            .device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .unwrap();
        rx.recv().unwrap().unwrap();
        {
            let actual = readback.get_mapped_range(..).unwrap();
            assert_eq!(
                &*actual, &expected,
                "concatenation and untouched tail for {chunks:?}"
            );
        }
        readback.unmap();
    }
}

#[test]
#[ignore = "requires a wgpu graphics device and a window system"]
fn pipeline_switches_preserve_camera_pixels() {
    let api = match std::env::var("DEADSYNC_WGPU_TEST_API").as_deref() {
        Ok("dx12") => Api::DirectX,
        Ok("opengl") => Api::OpenGL,
        Ok("vulkan") | Err(_) => Api::Vulkan,
        other => panic!("unknown wgpu test API: {other:?}"),
    };
    let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
    let window = Arc::new(
        event_loop
            .create_window(
                Window::default_attributes()
                    .with_visible(false)
                    .with_inner_size(PhysicalSize::new(96, 96)),
            )
            .unwrap(),
    );
    let fallback = Matrix4::from_translation([-0.1, 0.1, 0.0].into());
    let mut state = init(
        api,
        window,
        fallback,
        false,
        PresentModePolicy::Immediate,
        true,
    )
    .unwrap();
    assert!(
        state.config.usage.contains(wgpu::TextureUsages::COPY_SRC),
        "{api:?} surface does not support screenshot readback"
    );
    if api == Api::Vulkan {
        assert!(
            matches!(state.proj, ProjState::Immediates),
            "test must exercise immediates"
        );
    }
    check_buffer_uploads(&state);
    let rgba = RgbaImage::from_fn(4, 4, |x, y| {
        image::Rgba([180 + x as u8 * 20, 160 + y as u8 * 20, 230, 100])
    });
    let textures = TestTextures([
        create_texture(&mut state, &rgba, SamplerDesc::default()).unwrap(),
        create_yuv420_texture(
            &mut state,
            Yuv420Upload {
                width: 4,
                height: 4,
                y: &[
                    80, 120, 160, 200, 200, 160, 120, 80, 100, 140, 180, 220, 220, 180, 140, 100,
                ],
                u: &[100; 4],
                v: &[160; 4],
                levels: [0.0, 1.0, 0.5, 1.0],
                coeffs: [1.402, -0.344136, -0.714136, 1.772],
            },
            SamplerDesc::default(),
        )
        .unwrap(),
    ]);
    let mut cases = 0;
    for cached in [false, true] {
        for blend in [BlendMode::Alpha, BlendMode::Add] {
            for depth in [false, true] {
                for glow in [false, true] {
                    for target_alpha in [None, Some(false), Some(true)] {
                        let frame = interleaved_frame(blend, depth, glow, cached);
                        let reference = force_camera_uploads(&frame, fallback);
                        let (frame, reference) = if let Some(alpha) = target_alpha {
                            let mut frame = fixtures::offscreen(frame);
                            let mut reference = fixtures::offscreen(reference);
                            frame.render_targets[0].alpha = alpha;
                            reference.render_targets[0].alpha = alpha;
                            (frame, reference)
                        } else {
                            (frame, reference)
                        };
                        let expected = capture(&mut state, &reference, &textures);
                        let actual = capture(&mut state, &frame, &textures);
                        assert!(
                            actual.pixels().any(|pixel| pixel != actual.get_pixel(0, 0)),
                            "fixture must render visible content"
                        );
                        assert!(
                            actual.as_raw() == expected.as_raw(),
                            "{api:?} cached={cached} blend={blend:?} depth={depth} glow={glow} target_alpha={target_alpha:?}"
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    cleanup(&mut state);
    eprintln!("{api:?}: {cases} pipeline-switch pixel comparisons passed");
}
