use super::{WidgetBounds, WidgetLayout};

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
    pub(crate) fn new(widget: WidgetLayout) -> Self {
        let design_width = widget.bounds.width / widget.zoom;
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
