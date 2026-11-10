use super::{WidgetBounds, WidgetLayout};

const HEIGHT: f32 = 50.0;
const CENTER_Y_OFFSET: f32 = -55.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongDetailsLayout {
    pub widget: WidgetLayout,
    pub artist_label: WidgetLayout,
    pub artist_value: WidgetLayout,
    pub bpm_label: WidgetLayout,
    pub bpm_value: WidgetLayout,
    pub length_label: WidgetLayout,
    pub length_value: WidgetLayout,
    pub design_width: f32,
}

impl SongDetailsLayout {
    pub(crate) fn new(left_pane: WidgetLayout, screen_height: f32) -> Self {
        let design_width = left_pane.bounds.width / left_pane.zoom;
        let widget = WidgetLayout {
            bounds: WidgetBounds::new(
                left_pane.bounds.x,
                screen_height * 0.5 + CENTER_Y_OFFSET - HEIGHT * 0.5,
                left_pane.bounds.width,
                HEIGHT,
            ),
            zoom: left_pane.zoom,
        };
        let child = |x, y, width, height| widget.child(WidgetBounds::new(x, y, width, height));
        let length_label_x = design_width - 130.0;
        let length_value_x = design_width - 75.0;

        Self {
            widget,
            artist_label: child(6.0, 5.0, 44.0, 18.0),
            artist_value: child(55.0, 5.0, design_width - 60.0, 18.0),
            bpm_label: child(16.0, 27.0, 34.0, 18.0),
            bpm_value: child(55.0, 27.0, length_label_x - 55.0, 18.0),
            length_label: child(length_label_x, 27.0, 50.0, 18.0),
            length_value: child(length_value_x, 27.0, design_width - length_value_x, 18.0),
            design_width,
        }
    }
}
