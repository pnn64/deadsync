use super::{WidgetBounds, WidgetLayout};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongBannerLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
}

impl SongBannerLayout {
    pub(crate) fn new(widget: WidgetLayout) -> Self {
        Self {
            bounds: widget.bounds,
            zoom: widget.zoom,
        }
    }
}
