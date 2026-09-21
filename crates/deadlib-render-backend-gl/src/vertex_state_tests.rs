use super::{
    GlPath, State, Texture, TextureLookup, capture_frame, cleanup, create_texture, delete_texture,
    draw, init, request_screenshot,
};
use deadlib_render_core::{
    BlendMode, DrawOp, ProjectionMatrix, RenderFrame, SamplerDesc, TexturedMeshGeometry,
    TexturedMeshVertices,
};
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[allow(dead_code)]
#[path = "../../../tests/support/draw_batching.rs"]
mod fixtures;

struct TestTexture(Texture);

impl TextureLookup for TestTexture {
    fn opengl_texture(&self, handle: u64) -> Option<&Texture> {
        (handle == 7).then_some(&self.0)
    }
}

fn interleaved_frame(blend: BlendMode, depth: bool, glow: bool, storage: usize) -> RenderFrame {
    let sprites = fixtures::fixture(0, blend, false, glow);
    let meshes = fixtures::fixture(1, blend, false, false);
    let mut frame = fixtures::fixture(2, blend, depth, glow);
    let mut other_vertices = frame.tmesh_geometries[0].vertices.as_ref().to_vec();
    for vertex in &mut other_vertices {
        vertex.pos[0] *= 0.65;
        vertex.pos[1] *= 0.75;
        vertex.uv[0] = 1.0 - vertex.uv[0];
    }
    frame.tmesh_geometries.push(TexturedMeshGeometry {
        cache_key: 52,
        vertices: TexturedMeshVertices::Shared(other_vertices.into()),
    });
    // All retained, mixed retained/transient, and all transient buffers.
    for (index, geometry) in frame.tmesh_geometries.iter_mut().enumerate() {
        if storage == 2 || (storage == 1 && index == 1) {
            geometry.cache_key = 0;
        }
    }
    frame.sprite_instances = sprites.sprite_instances;
    frame.mesh_vertices = meshes.mesh_vertices;
    let textured_ops = std::mem::take(&mut frame.ops);
    for (index, geometry) in [0, 0, 1, 1, 0, 1].into_iter().enumerate() {
        let DrawOp::TexturedMesh(mut run) = textured_ops[index % textured_ops.len()] else {
            unreachable!();
        };
        run.geometry = geometry;
        frame.ops.push(DrawOp::TexturedMesh(run));
        frame.ops.push(if index % 2 == 0 {
            sprites.ops[index % sprites.ops.len()]
        } else {
            meshes.ops[index % meshes.ops.len()]
        });
    }
    frame
}

fn distinct_buffer_reference(frame: &RenderFrame) -> RenderFrame {
    let mut reference = frame.clone();
    reference.tmesh_geometries.clear();
    for op in &mut reference.ops {
        if let DrawOp::TexturedMesh(run) = op {
            let mut geometry = frame.tmesh_geometries[run.geometry as usize].clone();
            run.geometry = reference.tmesh_geometries.len() as u32;
            // Force a different retained buffer for each draw, so the reference
            // must rebind vertex attributes regardless of the preceding program.
            geometry.cache_key = 10_000 + u64::from(run.geometry);
            reference.tmesh_geometries.push(geometry);
        }
    }
    reference
}

fn capture(state: &mut State, frame: &RenderFrame, textures: &TestTexture) -> RgbaImage {
    request_screenshot(state);
    draw(state, frame, textures, false).expect("draw");
    capture_frame(state).expect("capture")
}

#[test]
#[ignore = "requires modern OpenGL and a window system"]
fn interleaved_vertex_buffers_preserve_pixels() {
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
    let mut state = init(window, ProjectionMatrix::IDENTITY, false, true, true).unwrap();
    assert_eq!(state.path, GlPath::Modern);
    let supports_base_instance = state.base_instance;
    let image = RgbaImage::from_fn(4, 4, |x, y| {
        Rgba([
            180 + x as u8 * 20,
            160 + y as u8 * 20,
            230,
            80 + (x + y) as u8 * 25,
        ])
    });
    let textures = TestTexture(create_texture(&state, &image, SamplerDesc::default()).unwrap());
    let mut cases = 0;
    for base_instance in [true, false] {
        if base_instance && !supports_base_instance {
            continue;
        }
        state.base_instance = base_instance;
        for storage in 0..3 {
            for blend in [BlendMode::Alpha, BlendMode::Add] {
                for depth in [false, true] {
                    for glow in [false, true] {
                        for target_alpha in [None, Some(false), Some(true)] {
                            let frame = interleaved_frame(blend, depth, glow, storage);
                            let reference = distinct_buffer_reference(&frame);
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
                                actual.pixels().any(|p| p != actual.get_pixel(0, 0)),
                                "fixture must render visible content"
                            );
                            assert!(
                                actual.as_raw() == expected.as_raw(),
                                "base_instance={base_instance} storage={storage} blend={blend:?} depth={depth} glow={glow} target_alpha={target_alpha:?}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    delete_texture(&state, &textures.0);
    cleanup(&mut state);
    eprintln!("{cases} interleaved vertex-buffer pixel comparisons passed");
}
