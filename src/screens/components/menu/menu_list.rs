use crate::act;
use crate::engine::present::actors::Actor;
use crate::engine::space::screen_center_x;
use crate::screens::components::shared::hitbox::HitRect;
use std::sync::Arc;

// --- CONSTANTS TO MATCH THE LUA SCRIPT'S STATIC STATE ---
const MENU_BASE_PX: f32 = 32.0; // An arbitrary base font size before zoom.
const FOCUS_ZOOM: f32 = 0.5; // Zoom factor when an item has focus.
const UNFOCUSED_ZOOM: f32 = 0.4; // Zoom factor when an item loses focus.

#[derive(Clone, Copy)]
pub struct MenuParams<'a> {
    pub options: &'a [Arc<str>],
    pub selected_index: usize,

    // In SM TL space:
    pub start_center_y: f32,
    pub row_spacing: f32,

    // Typography + colors
    pub selected_color: [f32; 4],
    pub normal_color: [f32; 4],
    pub font: &'static str,
}

/// Build a vertical, center-aligned menu with focus-based sizing and color.
pub fn build_vertical_menu(p: MenuParams) -> Vec<Actor> {
    let mut out = Vec::with_capacity(p.options.len());
    let center_x = screen_center_x();

    for (i, label) in p.options.iter().enumerate() {
        let is_selected = i == p.selected_index;

        // Determine zoom and color based on whether the item has focus.
        let zoom_factor = if is_selected {
            FOCUS_ZOOM
        } else {
            UNFOCUSED_ZOOM
        };
        let color = if is_selected {
            p.selected_color
        } else {
            p.normal_color
        };
        let center_y = (i as f32).mul_add(p.row_spacing, p.start_center_y);

        // Create a single, static text actor for each menu item.
        // The alpha is now taken directly from the color, ensuring it's visible.
        out.push(act!(text:
            align(0.5, 0.5):
            xy(center_x, center_y):
            zoomtoheight(MENU_BASE_PX * zoom_factor):
            diffuse(color[0], color[1], color[2], color[3]):
            shadowlength(0.8):
            font(p.font):
            settext(label.clone()):
            horizalign(center)
        ));
    }
    out
}

/// Compute mouse hit rectangles for the same rows `build_vertical_menu`
/// produces. Each rect is centred on `(screen_center_x(), start_center_y +
/// i*row_spacing)` with width `hit_width` and height `row_spacing`. Rect
/// `id` is the row index, so callers can recover it via `hit_test`.
///
/// Kept here (next to the actor builder) so layout and hit math share a
/// source of truth — drift between the two would show up as clicks
/// missing visible rows.
pub fn item_rects(
    count: usize,
    start_center_y: f32,
    row_spacing: f32,
    hit_width: f32,
) -> Vec<HitRect> {
    let mut rects = Vec::with_capacity(count);
    let center_x = screen_center_x();
    for i in 0..count {
        let center_y = (i as f32).mul_add(row_spacing, start_center_y);
        rects.push(HitRect::from_center(
            center_x,
            center_y,
            hit_width,
            row_spacing,
            i as u32,
        ));
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::item_rects;
    use crate::engine::space::{LogicalPos, ortho_for_window};
    use crate::screens::components::shared::hitbox::hit_test;

    #[test]
    fn item_rects_centered_on_screen_and_stacked_by_row_spacing() {
        ortho_for_window(854, 480);
        let rects = item_rects(3, 100.0, 28.0, 200.0);
        assert_eq!(rects.len(), 3);
        let cx = 0.5 * 854.0;
        for (i, r) in rects.iter().enumerate() {
            let expected_cy = (i as f32).mul_add(28.0, 100.0);
            assert!((0.5 * (r.min.x + r.max.x) - cx).abs() < 1e-3);
            assert!((0.5 * (r.min.y + r.max.y) - expected_cy).abs() < 1e-3);
            assert!((r.max.x - r.min.x - 200.0).abs() < 1e-3);
            assert!((r.max.y - r.min.y - 28.0).abs() < 1e-3);
            assert_eq!(r.id, i as u32);
        }
    }

    #[test]
    fn item_rects_hit_test_picks_correct_row() {
        ortho_for_window(854, 480);
        let rects = item_rects(3, 100.0, 28.0, 200.0);
        let cx = 0.5 * 854.0;
        for i in 0..3 {
            let cy = (i as f32).mul_add(28.0, 100.0);
            assert_eq!(hit_test(&rects, LogicalPos::new(cx, cy)), Some(i as u32));
        }
        assert_eq!(hit_test(&rects, LogicalPos::new(0.0, 0.0)), None);
    }
}
