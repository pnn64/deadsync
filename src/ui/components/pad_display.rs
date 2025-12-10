use crate::act;
use crate::ui::actors::{Actor, SizeSpec};

// This should match the native resolution of "rounded-square.png" from the theme (64x64).
const PANEL_NATIVE_SIZE: f32 = 64.0;
// Defines which panels are "active" for the dance-single layout.
const DANCE_LAYOUT: [bool; 9] = [false, true, false, true, false, true, false, true, false];
// Defines the layout for an inactive player.
const INACTIVE_LAYOUT: [bool; 9] = [
    false, false, false, false, false, false, false, false, false,
];

// Colors for active and inactive panels, matching the default (non-dark) theme.
const COLOR_USED: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const COLOR_UNUSED: [f32; 4] = [1.0, 1.0, 1.0, 0.3];

pub struct PadDisplayParams {
    pub center_x: f32,
    pub center_y: f32,
    pub zoom: f32,
    pub z: i16,
    pub is_active: bool,
}

/// Builds a 3x3 pad display actor, positioned and scaled as a group.
pub fn build(params: PadDisplayParams) -> Actor {
    let mut children = Vec::with_capacity(9);

    // Choose which layout to use based on whether the player is active.
    let layout = if params.is_active {
        DANCE_LAYOUT
    } else {
        INACTIVE_LAYOUT
    };

    // This is the final size of one panel after zoom.
    let zoomed_panel_size = PANEL_NATIVE_SIZE * params.zoom;

    // The Lua code positions panels relative to an origin where the center-bottom
    // panel (col=1, row=2) is at (0,0). We replicate this relative positioning by making the parent Frame center-aligned.
    for row in 0..3 {
        for col in 0..3 {
            let panel_index = row * 3 + col;
            let is_active = layout[panel_index];
            let color = if is_active { COLOR_USED } else { COLOR_UNUSED };

            // Position relative to the parent frame's center origin.
            // The distance between panel centers is exactly the size of one panel,
            // making them perfectly adjacent.
            let x = zoomed_panel_size * (col as f32 - 1.0);
            let y = zoomed_panel_size * (row as f32 - 2.0);

            children.push(
                act!(sprite("rounded-square.png"): // Use sprite instead of quad
                    align(0.5, 0.5): // The panel's center is its pivot point.
                    xy(x, y):
                    // The base size is set, then scaled by the zoom factor.
                    setsize(PANEL_NATIVE_SIZE, PANEL_NATIVE_SIZE):
                    zoom(params.zoom):
                    diffuse(color[0], color[1], color[2], color[3])
                ),
            );
        }
    }

    // The parent Frame groups all panels. Its origin is its center.
    Actor::Frame {
        align: [0.5, 0.5],                            // The frame's pivot is its center.
        offset: [params.center_x, params.center_y], // Position the center at this world coordinate.
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)], // Frame itself has no intrinsic size.
        children,
        background: None,
        z: params.z,
    }
}
