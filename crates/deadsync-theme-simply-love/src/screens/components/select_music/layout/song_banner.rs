use super::{WidgetBounds, WidgetLayout};

const DESIGN_WIDTH: f32 = 418.0;
const DESIGN_HEIGHT: f32 = 164.0;
const CENTER_Y: f32 = 96.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongBannerLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
}

impl SongBannerLayout {
    pub(crate) fn new(left_pane: WidgetLayout) -> Self {
        let zoom = left_pane.bounds.width / DESIGN_WIDTH;
        let widget = WidgetLayout::from_center(
            left_pane.bounds.center_x(),
            CENTER_Y,
            left_pane.bounds.width,
            DESIGN_HEIGHT * zoom,
            zoom,
        );

        Self {
            bounds: widget.bounds,
            zoom: widget.zoom,
        }
    }
}
