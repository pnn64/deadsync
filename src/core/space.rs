use cgmath::Matrix4;
use std::cell::Cell;

// -----------------------------------------------------------------------------
// Logical design space
// -----------------------------------------------------------------------------
#[inline(always)] pub const fn logical_height() -> f32 { 480.0 }
#[inline(always)] pub const fn design_width_16_9() -> f32 { 854.0 }

// -----------------------------------------------------------------------------
// Metrics (world space)
// -----------------------------------------------------------------------------
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub left:   f32,
    pub right:  f32,
    pub top:    f32,
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

#[allow(dead_code)]
#[inline(always)] pub fn screen_width()  -> f32 { CURRENT_METRICS.with(|c| { let m=c.get(); m.right - m.left }) }
#[allow(dead_code)]
#[inline(always)] pub fn screen_height() -> f32 { CURRENT_METRICS.with(|c| { let m=c.get(); m.top   - m.bottom }) }

// Top-left origin to match SM (SCREEN_LEFT/TOP = 0)
#[allow(dead_code)]
#[inline(always)] pub fn screen_left()   -> f32 { 0.0 }
#[allow(dead_code)]
#[inline(always)] pub fn screen_top()    -> f32 { 0.0 }
#[allow(dead_code)]
#[inline(always)] pub fn screen_right()  -> f32 { screen_width()  }
#[allow(dead_code)]
#[inline(always)] pub fn screen_bottom() -> f32 { screen_height() }

#[allow(dead_code)]
#[inline(always)] pub fn screen_center_x() -> f32 { 0.5 * screen_width()  }
#[allow(dead_code)]
#[inline(always)] pub fn screen_center_y() -> f32 { 0.5 * screen_height() }

// -----------------------------------------------------------------------------
// Metrics for a given window (pixels → world space, clamped ≤ 16:9)
// -----------------------------------------------------------------------------
#[inline(always)]
pub fn metrics_for_window(px_w: u32, px_h: u32) -> Metrics {
    let aspect = if px_h == 0 { 1.0 } else { px_w as f32 / px_h as f32 };
    let h = logical_height();                // 480 world units
    let w = if aspect >= 16.0/9.0 {
        // Match SM/SL exactly: 854 units at ≥16:9
        design_width_16_9()
    } else {
        // below 16:9, scale width from height
        (h * aspect).min(design_width_16_9())
    };
    let half_w = 0.5 * w;
    let half_h = 0.5 * h;

    Metrics {
        left: -half_w, right: half_w,
        bottom: -half_h, top: half_h,
    }
}


// -----------------------------------------------------------------------------
// Ortho for current window (also stores CURRENT_PIXEL + CURRENT_METRICS)
// -----------------------------------------------------------------------------
#[inline(always)]
pub fn ortho_for_window(width: u32, height: u32) -> Matrix4<f32> {
    set_current_window_px(width, height);
    let m = metrics_for_window(width, height);
    set_current_metrics(m);
    cgmath::ortho(m.left, m.right, m.bottom, m.top, -1.0, 1.0)
}

// -----------------------------------------------------------------------------
// Aspect helpers
// -----------------------------------------------------------------------------
#[inline(always)]
pub fn is_wide() -> bool {
    let w = screen_width();
    let h = screen_height();
    if h <= 0.0 { return true; } // Avoid div by zero; default to wide
    (w / h) >= 1.6
}

// -----------------------------------------------------------------------------
// WideScale helpers
// -----------------------------------------------------------------------------

/// Helper to select a scale factor based on screen aspect ratio.
pub fn widescale(n43: f32, n169: f32) -> f32 {
    if is_wide() { n169 } else { n43 }
}