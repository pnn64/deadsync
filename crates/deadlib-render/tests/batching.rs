#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use deadlib_render::{Backend, Texture, create_backend};
use deadlib_render_core::*;
use image::{Rgba, RgbaImage};
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[path = "../../../tests/support/draw_batching.rs"]
mod fixtures;
use fixtures::{coalesce, fixture, offscreen};

fn capture(
    backend: &mut Backend,
    f: &RenderFrame,
    textures: &TextureHandleMap<Texture>,
) -> RgbaImage {
    backend.request_screenshot();
    backend.draw(f, textures, false).expect("draw");
    backend.capture_frame().expect("capture")
}

#[test]
#[ignore = "requires graphics devices and a window system; compares pixels within each backend"]
fn compatible_batches_preserve_backend_pixels() {
    let event_loop = EventLoop::builder().with_any_thread(true).build().unwrap();
    // Select one graphics API per process to isolate device lifetimes.
    let selected = std::env::var("DEADSYNC_BATCH_BACKEND").unwrap_or_else(|_| "opengl".into());
    let backend_kind = selected.parse::<BackendType>().unwrap();
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
    let mut backend = create_backend(
        backend_kind,
        window,
        ProjectionMatrix::IDENTITY,
        false,
        PresentModePolicy::Immediate,
        false,
        true,
    )
    .expect("graphics backend");
    let mut textures = TextureHandleMap::default();
    let image = RgbaImage::from_fn(4, 4, |x, y| {
        Rgba([
            180 + x as u8 * 20,
            160 + y as u8 * 20,
            230,
            80 + (x + y) as u8 * 25,
        ])
    });
    textures.insert(
        7,
        backend
            .create_texture(&image, SamplerDesc::default())
            .unwrap(),
    );
    let mut cases = 0;
    for kind in 0..3 {
        for blend in [BlendMode::Alpha, BlendMode::Add] {
            for depth in [false, true] {
                if depth && kind != 2 {
                    continue;
                }
                for glow in [false, true] {
                    if glow && kind == 1 {
                        continue;
                    }
                    for target in [false, true] {
                        let original = fixture(kind, blend, depth, glow);
                        let combined = coalesce(&original);
                        let (original, combined) = if target {
                            (offscreen(original), offscreen(combined))
                        } else {
                            (original, combined)
                        };
                        let before = capture(&mut backend, &original, &textures);
                        let after = capture(&mut backend, &combined, &textures);
                        assert!(
                            before.pixels().any(|p| p != before.get_pixel(0, 0)),
                            "fixture must render visible content: {backend_kind} kind={kind} depth={depth} target={target}"
                        );
                        assert!(
                            before.as_raw() == after.as_raw(),
                            "{backend_kind} kind={kind} blend={blend:?} depth={depth} glow={glow} target={target}"
                        );
                        // Native Vulkan currently ignores the depth_test flag.
                        if depth && backend_kind != BackendType::Vulkan {
                            let without_depth = fixture(kind, blend, false, glow);
                            let without_depth = if target {
                                offscreen(without_depth)
                            } else {
                                without_depth
                            };
                            assert!(
                                before.as_raw()
                                    != capture(&mut backend, &without_depth, &textures).as_raw(),
                                "fixture must exercise depth rejection: {backend_kind} target={target}"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    eprintln!("{backend_kind}: {cases} before/after pixel comparisons passed");
    drop(textures);
    backend.cleanup();
}
