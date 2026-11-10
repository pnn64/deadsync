use super::{DensityGraphLayout, WidgetBounds};

#[derive(Clone, Copy)]
pub(crate) enum StepArtistBarY {
    Default,
    Expanded,
}

impl From<bool> for StepArtistBarY {
    fn from(expanded: bool) -> Self {
        if expanded {
            Self::Expanded
        } else {
            Self::Default
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepArtistPeakNpsLayout {
    pub step_header_y: f32,
    pub artist_background_width: f32,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub peak_bounds: WidgetBounds,
    pub peak_text_zoom: f32,
    pub peak_text_max_width: f32,
}

impl StepArtistPeakNpsLayout {
    pub(crate) fn for_graph(graph: DensityGraphLayout, peak_text_logical_width: f32) -> Self {
        let panel = graph.bounds;
        let peak_text_zoom = 0.8 * graph.zoom;
        let peak_width = (peak_text_logical_width * peak_text_zoom).clamp(0.0, panel.width);
        let peak_bounds = WidgetBounds::new(
            panel.width - peak_width,
            graph.step_artist_header_y - panel.y,
            peak_width,
            0.0,
        );
        let gap = 8.0 * graph.zoom;
        let artist_background_width = (panel.width - peak_width - gap).max(0.0);
        let artist_max_width =
            (artist_background_width - graph.artist_x_offset - gap).max(0.0) * graph.zoom;

        Self {
            step_header_y: graph.step_artist_header_y,
            artist_background_width,
            artist_x_offset: graph.artist_x_offset,
            artist_max_width,
            peak_bounds,
            peak_text_zoom,
            peak_text_max_width: peak_width / peak_text_zoom,
        }
    }
}
