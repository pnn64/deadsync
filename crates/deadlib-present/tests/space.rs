use deadlib_present::{
    actors::RenderTarget,
    compose::{ActorSegment, ComposeScratch, TextLayoutCache, build_passes},
    font::FontMap,
    space::{self, Metrics},
    texture::NullTextureContext,
};
use deadlib_render_core::render_target_texture_handle;
use glam::Vec3;
use std::sync::Arc;

#[test]
fn projection_maps_arbitrary_bounds_to_clip_edges() {
    for metrics in [
        Metrics::centered(320.0, 200.0),
        Metrics::centered(1200.0, 300.0),
        Metrics {
            left: 10.0,
            right: 30.0,
            bottom: -40.0,
            top: 20.0,
        },
    ] {
        let projection = metrics.projection();
        for (world, clip) in [
            (
                Vec3::new(metrics.left, metrics.bottom, 0.0),
                Vec3::new(-1.0, -1.0, 0.0),
            ),
            (
                Vec3::new(metrics.right, metrics.top, 0.0),
                Vec3::new(1.0, 1.0, 0.0),
            ),
        ] {
            assert!(projection.transform_point3(world).abs_diff_eq(clip, 1e-6));
        }
    }
}

#[test]
fn pixel_resize_preserves_logical_bounds() {
    space::set_current_metrics(Metrics::centered(320.0, 200.0));
    for (width, height) in [(640, 480), (1920, 1080), (3840, 780), (0, 0)] {
        space::set_current_window_px(width, height);
        assert_eq!(space::current_window_px(), (width, height));
        assert_eq!(
            (space::screen_width(), space::screen_height()),
            (320.0, 200.0)
        );
        assert_eq!(
            (space::screen_center_x(), space::screen_center_y()),
            (160.0, 100.0)
        );
    }
}

#[test]
fn offscreen_camera_uses_logical_bounds_without_window_centering() {
    let metrics = Metrics::centered(320.0, 200.0);
    space::set_current_window_px(1000, 500);
    space::set_overscan(50, 25, 100, 50);
    let targets = [[1024, 64], [64, 1024], [0, 0]].map(|size| RenderTarget {
        texture_handle: render_target_texture_handle(u64::from(size[0]) + 1),
        size,
        logical_size: [40.0, 20.0],
        alpha: true,
        depth: false,
        preserve: false,
        children: Arc::from([]),
    });
    let frame = build_passes(
        std::iter::once(ActorSegment::new(&[])),
        &targets,
        [0.0; 4],
        &metrics,
        &FontMap::default(),
        0.0,
        &mut TextLayoutCache::default(),
        &mut ComposeScratch::default(),
        &NullTextureContext,
        None,
    );
    space::set_overscan(0, 0, 0, 0);
    let corner = frame.cameras[0].transform_point3(Vec3::new(160.0, 100.0, 0.0));
    assert!(corner.abs_diff_eq(Vec3::new(1.2, 1.0, 0.0), 1e-6));
    for (target, pass) in targets.iter().zip(&frame.render_targets) {
        assert_eq!(
            (pass.width, pass.height),
            (target.size[0].max(1), target.size[1].max(1))
        );
        let corner = pass.cameras[0].transform_point3(Vec3::new(20.0, 10.0, 0.0));
        assert!(corner.abs_diff_eq(Vec3::new(1.0, 1.0, 0.0), 1e-6));
    }
}
