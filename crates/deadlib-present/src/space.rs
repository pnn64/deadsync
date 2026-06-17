use glam::Mat4 as Matrix4;
use glam::Vec3;
use std::cell::Cell;
use std::sync::atomic::{AtomicI32, Ordering};

// -----------------------------------------------------------------------------
// Logical design space
// -----------------------------------------------------------------------------
#[inline(always)]
pub const fn logical_height() -> f32 {
    480.0
}
#[inline(always)]
pub const fn design_width_16_9() -> f32 {
    854.0
}

// -----------------------------------------------------------------------------
// Metrics (world space)
// -----------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

// Thread-local current metrics and current *pixel* size
thread_local! {
    static CURRENT_METRICS: Cell<Metrics> = Cell::new(default_metrics());
    static CURRENT_PIXEL:   Cell<(u32,u32)> = const { Cell::new((854, 480)) };
}

#[inline(always)]
fn default_metrics() -> Metrics {
    // sensible default (16:9 design space), used only until the app sets real metrics
    metrics_for_window(854, 480)
}

#[inline(always)]
pub fn set_current_metrics(m: Metrics) {
    CURRENT_METRICS.with(|c| c.set(m));
}

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

/// Current live overscan values: (translate_x, translate_y, add_width, add_height).
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
pub fn current_centering_matrix() -> Option<Matrix4> {
    let (tx, ty, aw, ah) = overscan();
    if (tx, ty, aw, ah) == (0, 0, 0, 0) {
        return None;
    }
    let (pw, ph) = current_window_px();
    Some(centering_matrix(tx, ty, aw, ah, pw, ph))
}

#[allow(dead_code)]
#[inline(always)]
pub fn screen_width() -> f32 {
    CURRENT_METRICS.with(|c| {
        let m = c.get();
        m.right - m.left
    })
}
#[allow(dead_code)]
#[inline(always)]
pub fn screen_height() -> f32 {
    CURRENT_METRICS.with(|c| {
        let m = c.get();
        m.top - m.bottom
    })
}

// Top-left origin to match SM (SCREEN_LEFT/TOP = 0)
#[allow(dead_code)]
#[inline(always)]
pub const fn screen_left() -> f32 {
    0.0
}
#[allow(dead_code)]
#[inline(always)]
pub const fn screen_top() -> f32 {
    0.0
}
#[allow(dead_code)]
#[inline(always)]
pub fn screen_right() -> f32 {
    screen_width()
}
#[allow(dead_code)]
#[inline(always)]
pub fn screen_bottom() -> f32 {
    screen_height()
}

#[allow(dead_code)]
#[inline(always)]
pub fn screen_center_x() -> f32 {
    0.5 * screen_width()
}
#[allow(dead_code)]
#[inline(always)]
pub fn screen_center_y() -> f32 {
    0.5 * screen_height()
}

// -----------------------------------------------------------------------------
// Metrics for a given window (pixels → world space, clamped ≤ 16:9)
// -----------------------------------------------------------------------------
#[inline(always)]
pub fn metrics_for_window(px_w: u32, px_h: u32) -> Metrics {
    let aspect = if px_h == 0 {
        1.0
    } else {
        px_w as f32 / px_h as f32
    };
    let h = logical_height(); // 480 world units
    let w = if aspect >= 16.0 / 9.0 {
        // Match SM/SL exactly: 854 units at ≥16:9
        design_width_16_9()
    } else {
        // below 16:9, scale width from height
        (h * aspect).min(design_width_16_9())
    };
    let half_w = 0.5 * w;
    let half_h = 0.5 * h;

    Metrics {
        left: -half_w,
        right: half_w,
        bottom: -half_h,
        top: half_h,
    }
}

// -----------------------------------------------------------------------------
// Ortho for current window (also stores CURRENT_PIXEL + CURRENT_METRICS)
// -----------------------------------------------------------------------------
#[inline(always)]
pub fn ortho_for_window(width: u32, height: u32) -> Matrix4 {
    set_current_window_px(width, height);
    let m = metrics_for_window(width, height);
    set_current_metrics(m);
    Matrix4::orthographic_rh_gl(m.left, m.right, m.bottom, m.top, -1.0, 1.0)
}

// -----------------------------------------------------------------------------
// Aspect helpers
// -----------------------------------------------------------------------------
#[inline(always)]
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
pub fn widescale(n43: f32, n169: f32) -> f32 {
    if is_wide() { n169 } else { n43 }
}

#[cfg(test)]
mod tests {
    use super::*;

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
