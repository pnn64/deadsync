#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WidgetBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl WidgetBounds {
    pub(super) const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center_x(self) -> f32 {
        self.x + self.width * 0.5
    }

    pub fn center_y(self) -> f32 {
        self.y + self.height * 0.5
    }

    pub(super) fn translated_y(mut self, offset: f32) -> Self {
        self.y += offset;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WidgetLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
}

impl WidgetLayout {
    pub(super) fn translated_y(mut self, offset: f32) -> Self {
        self.bounds = self.bounds.translated_y(offset);
        self
    }

    pub(super) fn child(self, bounds: WidgetBounds) -> Self {
        let zoom = self.zoom;
        Self {
            bounds: WidgetBounds::new(
                bounds.x * zoom,
                bounds.y * zoom,
                bounds.width * zoom,
                bounds.height * zoom,
            ),
            zoom,
        }
    }

    pub(super) fn from_edges(left: f32, top: f32, right: f32, bottom: f32, zoom: f32) -> Self {
        Self {
            bounds: WidgetBounds::new(left, top, right - left, bottom - top),
            zoom,
        }
    }
}
