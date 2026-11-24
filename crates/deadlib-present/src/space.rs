//! Logical presentation bounds, pixel dimensions, and clip-space centering.
//!
//! Applications install their logical bounds and surface dimensions independently.
//! Until configured, the current bounds span -1 to 1 and the pixel size is zero.

use glam::Mat4 as Matrix4;
use glam::Vec3;
use std::cell::Cell;
use std::sync::atomic::{AtomicI32, Ordering};

// -----------------------------------------------------------------------------
// Metrics (world space)
// -----------------------------------------------------------------------------
/// Logical world-space bounds used to construct an orthographic camera.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Metrics {
    /// Constructs logical bounds centered on the world origin.
    #[must_use]
    pub const fn centered(width: f32, height: f32) -> Self {
        Self {
            left: -0.5 * width,
            right: 0.5 * width,
            bottom: -0.5 * height,
            top: 0.5 * height,
        }
    }

    /// Projects these bounds into OpenGL clip space, with depth from -1 to 1.
    ///
    /// Bounds must be finite, with positive width and height.
    #[must_use]
    pub fn projection(self) -> Matrix4 {
        glam::camera::rh::proj::opengl::orthographic(
            self.left,
            self.right,
            self.bottom,
            self.top,
            -1.0,
            1.0,
        )
    }
}

/// Perspective for a top-left layout plane. Objects at Z=0 keep their layout
/// positions, and positive Z moves toward the eye. The field of view is horizontal.
#[must_use]
pub fn menu_perspective(
    [width, height]: [f32; 2],
    [vanish_x, vanish_y]: [f32; 2],
    horizontal_fov_deg: f32,
) -> Option<(Matrix4, Matrix4)> {
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let half_w = 0.5 * width;
    let half_h = 0.5 * height;
    let tan_theta = (0.5 * horizontal_fov_deg).to_radians().tan();
    if !tan_theta.is_finite() || tan_theta.abs() < 1e-6 {
        return None;
    }
    let dist = half_w / tan_theta;
    if !dist.is_finite() || dist <= 0.0 {
        return None;
    }

    let reverse_viewport = |at: f32, size: f32| {
        if size < 1e-6 {
            0.0
        } else {
            (at / size).mul_add(-size, size)
        }
    };
    let vp_x = reverse_viewport(vanish_x, width) - half_w;
    let vp_y = reverse_viewport(vanish_y, height) - half_h;
    let left = (vp_x - half_w) / dist;
    let right = (vp_x + half_w) / dist;
    let bottom = (vp_y + half_h) / dist;
    let top = (vp_y - half_h) / dist;
    let near = 1.0_f32;
    let far = dist + 1000.0_f32;
    let projection = Matrix4::from_cols_array(&[
        2.0 * near / (right - left),
        0.0,
        0.0,
        0.0,
        0.0,
        2.0 * near / (top - bottom),
        0.0,
        0.0,
        (right + left) / (right - left),
        (top + bottom) / (top - bottom),
        -(far + near) / (far - near),
        -1.0,
        0.0,
        0.0,
        -2.0 * far * near / (far - near),
        0.0,
    ]);
    let eye = Vec3::new(-vp_x + half_w, -vp_y + half_h, dist);
    let at = Vec3::new(eye.x, eye.y, 0.0);
    Some((
        projection,
        glam::camera::rh::view::look_at_mat4(eye, at, Vec3::Y),
    ))
}

// Logical bounds and surface pixels are independent, caller-owned choices.
thread_local! {
    static CURRENT_METRICS: Cell<Metrics> = const { Cell::new(Metrics::centered(2.0, 2.0)) };
    static CURRENT_PIXEL: Cell<(u32, u32)> = const { Cell::new((0, 0)) };
}

/// Installs logical bounds for presentation on the calling thread.
#[inline(always)]
pub fn set_current_metrics(m: Metrics) {
    CURRENT_METRICS.with(|c| c.set(m));
}

/// Returns the exact logical bounds installed on the calling thread.
#[inline(always)]
#[must_use]
pub fn current_metrics() -> Metrics {
    CURRENT_METRICS.with(Cell::get)
}

/// Runs a presentation operation in temporary logical bounds, restoring the
/// calling thread's exact previous bounds on return or unwind. Surface pixels
/// and overscan remain independent of this logical camera scope.
pub fn with_current_metrics<R>(metrics: Metrics, operation: impl FnOnce() -> R) -> R {
    struct Restore(Metrics);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_current_metrics(self.0);
        }
    }
    let previous = CURRENT_METRICS.with(|current| current.replace(metrics));
    let _restore = Restore(previous);
    operation()
}

/// Updates surface pixels without changing the current logical bounds.
#[inline(always)]
pub fn set_current_window_px(px_w: u32, px_h: u32) {
    CURRENT_PIXEL.with(|c| c.set((px_w, px_h)));
}

#[inline(always)]
pub fn current_window_px() -> (u32, u32) {
    CURRENT_PIXEL.with(std::cell::Cell::get)
}

// -----------------------------------------------------------------------------
// Overscan adjustment (CenterImage)
//
// Four values in physical *window pixels* that scale/translate the entire
// rendered image. Stored in lock-free atomics so the input thread (overscan
// screen) can live-preview edits while the render thread reads them every frame.
// -----------------------------------------------------------------------------
static OVERSCAN_TRANSLATE_X: AtomicI32 = AtomicI32::new(0);
static OVERSCAN_TRANSLATE_Y: AtomicI32 = AtomicI32::new(0);
static OVERSCAN_ADD_WIDTH: AtomicI32 = AtomicI32::new(0);
static OVERSCAN_ADD_HEIGHT: AtomicI32 = AtomicI32::new(0);

/// Set the live overscan values (does not persist to disk).
#[inline]
pub fn set_overscan(translate_x: i32, translate_y: i32, add_width: i32, add_height: i32) {
    OVERSCAN_TRANSLATE_X.store(translate_x, Ordering::Relaxed);
    OVERSCAN_TRANSLATE_Y.store(translate_y, Ordering::Relaxed);
    OVERSCAN_ADD_WIDTH.store(add_width, Ordering::Relaxed);
    OVERSCAN_ADD_HEIGHT.store(add_height, Ordering::Relaxed);
}

/// Current live overscan values: (`translate_x`, `translate_y`, `add_width`, `add_height`).
#[inline]
pub fn overscan() -> (i32, i32, i32, i32) {
    (
        OVERSCAN_TRANSLATE_X.load(Ordering::Relaxed),
        OVERSCAN_TRANSLATE_Y.load(Ordering::Relaxed),
        OVERSCAN_ADD_WIDTH.load(Ordering::Relaxed),
        OVERSCAN_ADD_HEIGHT.load(Ordering::Relaxed),
    )
}

/// True if any overscan value is non-zero (i.e. centering should be applied).
#[inline]
#[must_use]
pub fn overscan_active() -> bool {
    overscan() != (0, 0, 0, 0)
}

/// Minimum effective scale, so an extreme negative AddWidth/AddHeight can't make
/// the image vanish or invert.
const MIN_OVERSCAN_SCALE: f32 = 0.05;

/// Pure centering matrix used by the renderer. `pw`/`ph` are the physical window
/// dimensions in pixels. The matrix is applied in clip space (NDC, edges -1..1)
/// by post-multiplying onto each camera: `camera = C * camera`.
///
/// ```text
/// shiftX =  2*tx/pw   scaleX = 1 + aw/pw
/// shiftY = -2*ty/ph   scaleY = 1 + ah/ph
/// C = translate(shiftX, shiftY) * scale(scaleX, scaleY)
/// ```
#[inline]
#[must_use]
pub fn centering_matrix(tx: i32, ty: i32, aw: i32, ah: i32, pw: u32, ph: u32) -> Matrix4 {
    let pw = pw.max(1) as f32;
    let ph = ph.max(1) as f32;
    let shift_x = 2.0 * tx as f32 / pw;
    let shift_y = -2.0 * ty as f32 / ph;
    let scale_x = (1.0 + aw as f32 / pw).max(MIN_OVERSCAN_SCALE);
    let scale_y = (1.0 + ah as f32 / ph).max(MIN_OVERSCAN_SCALE);
    Matrix4::from_translation(Vec3::new(shift_x, shift_y, 0.0))
        * Matrix4::from_scale(Vec3::new(scale_x, scale_y, 1.0))
}

/// Centering matrix for the current live overscan values and window size, or
/// `None` when no adjustment is active.
#[inline]
#[must_use]
pub fn current_centering_matrix() -> Option<Matrix4> {
    let (tx, ty, aw, ah) = overscan();
    if (tx, ty, aw, ah) == (0, 0, 0, 0) {
        return None;
    }
    let (pw, ph) = current_window_px();
    Some(centering_matrix(tx, ty, aw, ah, pw, ph))
}

#[inline(always)]
#[must_use]
pub fn screen_width() -> f32 {
    CURRENT_METRICS.with(|c| {
        let m = c.get();
        m.right - m.left
    })
}
#[inline(always)]
#[must_use]
pub fn screen_height() -> f32 {
    CURRENT_METRICS.with(|c| {
        let m = c.get();
        m.top - m.bottom
    })
}

// Presentation coordinates use a top-left origin.
#[inline(always)]
#[must_use]
pub const fn screen_left() -> f32 {
    0.0
}
#[inline(always)]
#[must_use]
pub const fn screen_top() -> f32 {
    0.0
}
#[inline(always)]
#[must_use]
pub fn screen_right() -> f32 {
    screen_width()
}
#[inline(always)]
#[must_use]
pub fn screen_bottom() -> f32 {
    screen_height()
}

#[inline(always)]
#[must_use]
pub fn screen_center_x() -> f32 {
    0.5 * screen_width()
}
#[inline(always)]
#[must_use]
pub fn screen_center_y() -> f32 {
    0.5 * screen_height()
}

// -----------------------------------------------------------------------------
// Aspect helpers
// -----------------------------------------------------------------------------
#[inline(always)]
#[must_use]
pub fn is_wide() -> bool {
    let w = screen_width();
    let h = screen_height();
    if h <= 0.0 {
        return true;
    } // Avoid div by zero; default to wide
    (w / h) >= 1.6
}

// -----------------------------------------------------------------------------
// WideScale helpers
// -----------------------------------------------------------------------------

/// Helper to select a scale factor based on screen aspect ratio.
#[must_use]
pub fn widescale(n43: f32, n169: f32) -> f32 {
    if is_wide() { n169 } else { n43 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_metrics_restore_exact_bounds_after_nested_return_and_unwind() {
        fn bits(m: Metrics) -> [u32; 4] {
            [m.left, m.right, m.top, m.bottom].map(f32::to_bits)
        }
        let original = Metrics {
            left: -123.5,
            right: 730.5,
            top: 208.25,
            bottom: -271.75,
        };
        let target = Metrics::centered(8.0, 6.0);
        let pixels = current_window_px();
        with_current_metrics(original, || {
            let value = with_current_metrics(target, || {
                assert_eq!(bits(current_metrics()), bits(target));
                let panic = std::panic::catch_unwind(|| {
                    with_current_metrics(Metrics::centered(3.0, 2.0), || {
                        panic!("render callback failed");
                    });
                });
                assert!(panic.is_err());
                assert_eq!(bits(current_metrics()), bits(target));
                assert_eq!(current_window_px(), pixels);
                42
            });
            assert_eq!(value, 42);
            assert_eq!(bits(current_metrics()), bits(original));
        });
    }

    #[test]
    fn centering_matrix_zero_is_identity() {
        let c = centering_matrix(0, 0, 0, 0, 1920, 1080);
        assert_eq!(c, Matrix4::IDENTITY);
    }

    #[test]
    fn centering_matrix_known_values() {
        let (pw, ph) = (1920u32, 1080u32);
        let c = centering_matrix(96, 54, 192, 108, pw, ph);
        // scaleX = 1 + 192/1920 = 1.1, scaleY = 1 + 108/1080 = 1.1
        // shiftX = 2*96/1920 = 0.1, shiftY = -2*54/1080 = -0.1
        let expected = Matrix4::from_translation(Vec3::new(0.1, -0.1, 0.0))
            * Matrix4::from_scale(Vec3::new(1.1, 1.1, 1.0));
        let a = c.to_cols_array();
        let b = expected.to_cols_array();
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x - y).abs() < 1e-6, "{x} != {y}");
        }
    }

    #[test]
    fn centering_matrix_clamps_negative_scale() {
        // AddWidth/AddHeight far more negative than the window → clamp to floor.
        let c = centering_matrix(0, 0, -10000, -10000, 1920, 1080);
        let cols = c.to_cols_array();
        // scaleX is column 0 row 0, scaleY is column 1 row 1.
        assert!((cols[0] - MIN_OVERSCAN_SCALE).abs() < 1e-6);
        assert!((cols[5] - MIN_OVERSCAN_SCALE).abs() < 1e-6);
    }
}
