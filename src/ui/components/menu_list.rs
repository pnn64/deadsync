use crate::act;
use crate::core::space::*;
use crate::ui::actors::Actor;

// --- CONSTANTS TO MATCH THE LUA SCRIPT'S STATIC STATE ---
const MENU_BASE_PX: f32 = 32.0; // An arbitrary base font size before zoom.
const FOCUS_ZOOM: f32 = 0.5; // Zoom factor when an item has focus.
const UNFOCUSED_ZOOM: f32 = 0.4; // Zoom factor when an item loses focus.

#[derive(Clone, Copy)]
pub struct MenuParams<'a> {
    pub options: &'a [&'a str],
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
        let center_y = p.start_center_y + (i as f32) * p.row_spacing;

        // Create a single, static text actor for each menu item.
        // The alpha is now taken directly from the color, ensuring it's visible.
        out.push(act!(text:
            align(0.5, 0.5):
            xy(center_x, center_y):
            zoomtoheight(MENU_BASE_PX):
            zoom(zoom_factor):
            diffuse(color[0], color[1], color[2], color[3]):
            shadowlength(0.8):
            font(p.font):
            settext(*label):
            horizalign(center)
        ));
    }
    out
}
