#![cfg(all(
    target_os = "windows",
    not(target_pointer_width = "32"),
    not(target_vendor = "win7")
))]

use deadlib_render::{Backend, create_backend};
use deadlib_render_core::{
    BackendType, BlendMode, DrawOp, MeshRun, MeshVertex, PresentModePolicy, ProjectionMatrix,
    RenderFrame, TextureHandleMap,
};
use std::sync::Arc;
use winit::{
    dpi::PhysicalSize, event_loop::EventLoop, platform::windows::EventLoopBuilderExtWindows,
    window::Window,
};

#[test]
#[ignore = "requires native OpenGL and Vulkan devices and a window system"]
fn gpu_resize_preserves_caller_projection() {
    let event_loop = EventLoop::builder()
        .with_any_thread(true)
        .build()
        .expect("event loop");
    let frame = RenderFrame {
        clear_color: [0.0, 0.0, 0.0, 1.0],
        render_targets: Vec::new(),
        cameras: Vec::new(),
        sprite_instances: Vec::new(),
        mesh_vertices: [
            (-5.0, -2.5),
            (5.0, -2.5),
            (5.0, 2.5),
            (-5.0, -2.5),
            (5.0, 2.5),
            (-5.0, 2.5),
        ]
        .map(|(x, y)| MeshVertex {
            pos: [x, y],
            color: [1.0; 4],
        })
        .to_vec(),
        tmesh_instances: Vec::new(),
        tmesh_geometries: Vec::new(),
        ops: vec![DrawOp::Mesh(MeshRun {
            vertex_start: 0,
            vertex_count: 6,
            blend: BlendMode::Alpha,
            camera: 0,
        })],
    };
    // A 20x10 logical canvas, independent of the window's physical aspect.
    let projection = ProjectionMatrix::from_cols_array(&[
        0.1, 0.0, 0.0, 0.0, 0.0, 0.2, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);
    for kind in [BackendType::OpenGL, BackendType::Vulkan] {
        #[expect(deprecated, reason = "hidden renderer fixture needs no event dispatch")]
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(PhysicalSize::new(80, 60)),
                )
                .expect("hidden window"),
        );
        let mut backend = create_backend(
            kind,
            Arc::clone(&window),
            projection,
            false,
            PresentModePolicy::Immediate,
            false,
            true,
        )
        .expect("graphics backend");
        assert_coverage(&mut backend, &frame, 4);
        // Explicitly changing the camera must survive minimization and later resizes.
        let mut custom = projection.to_cols_array();
        custom[0] *= 0.5;
        custom[5] *= 0.5;
        backend.set_default_projection(ProjectionMatrix::from_cols_array(&custom));
        backend.resize(0, 0);
        for (width, height) in [(120, 80), (160, 90), (240, 60)] {
            let size = window
                .request_inner_size(PhysicalSize::new(width, height))
                .unwrap_or_else(|| window.inner_size());
            backend.resize(size.width, size.height);
            assert_coverage(&mut backend, &frame, 8);
        }
        backend.cleanup();
    }
}

fn assert_coverage(backend: &mut Backend, frame: &RenderFrame, inset: u32) {
    backend.request_screenshot();
    backend
        .draw(frame, &TextureHandleMap::default(), false)
        .expect("draw frame");
    let image = backend.capture_frame().expect("capture frame");
    let (width, height) = image.dimensions();
    assert!(width > 0 && height > 0);
    // Probe safely inside and outside the expected edges, avoiding raster edge rounding.
    for (x, y, white) in [
        (width / 2, height / 2, true),
        (width / 2 + width / inset - 2, height / 2, true),
        (width / 2 + width / inset + 2, height / 2, false),
        (width / 2, height / 2 + height / inset - 2, true),
        (width / 2, height / 2 + height / inset + 2, false),
    ] {
        assert_eq!(
            image.get_pixel(x, y).0,
            if white { [255; 4] } else { [0, 0, 0, 255] },
            "coverage at {x},{y} on {width}x{height}, inset {inset}"
        );
    }
}
