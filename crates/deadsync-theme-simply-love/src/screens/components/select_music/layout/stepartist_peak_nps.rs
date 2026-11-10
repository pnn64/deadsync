use super::WidgetBounds;

// We don't want any text to touch the edges of any panel
const TEXT_PADDING: f32 = 8.0;

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

#[derive(Clone, Copy)]
struct StepArtistPeakNpsOffsets {
    header_gap: f32,
    header_screen_height_scale: f32,
    artist_x: f32,
}

#[derive(Clone, Copy)]
struct StepArtistPeakNpsOffsetsByMode {
    default: StepArtistPeakNpsOffsets,
    expanded: StepArtistPeakNpsOffsets,
}

impl StepArtistPeakNpsOffsetsByMode {
    fn get(self, mode: StepArtistBarY) -> StepArtistPeakNpsOffsets {
        match mode {
            StepArtistBarY::Default => self.default,
            StepArtistBarY::Expanded => self.expanded,
        }
    }
}

const OFFSETS: StepArtistPeakNpsOffsetsByMode = StepArtistPeakNpsOffsetsByMode {
    default: StepArtistPeakNpsOffsets {
        header_gap: 32.0,
        header_screen_height_scale: 1.0 / 56.0,
        artist_x: 49.5,
    },
    expanded: StepArtistPeakNpsOffsets {
        header_gap: 80.0,
        header_screen_height_scale: 0.0,
        artist_x: 45.0,
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepArtistPeakNpsLayout {
    pub step_header_y: f32,
    pub artist_background_width: f32,
    pub artist_x_offset: f32,
    pub artist_max_width: f32,
    pub nps_text_bounds: WidgetBounds,
    pub nps_text_zoom: f32,
    pub nps_text_max_width: f32,
}

// This layoutfigures out the horizontal spacing for the step artist and peak NPS elements within the graph panel.
// We first measure the peak nps text, then allow the panel to
// take up the remaining horizontal space for the step artist elements.
impl StepArtistPeakNpsLayout {
    pub(crate) fn for_graph(
        graph_bounds: WidgetBounds,
        graph_zoom: f32,
        screen_height: f32,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> Self {
        let offsets = OFFSETS.get(mode);
        let graph_center_y = graph_bounds.center_y();
        let step_header_y = graph_center_y
            - offsets.header_gap
            - screen_height * offsets.header_screen_height_scale;
        let nps_text_zoom = 0.8 * graph_zoom;
        let panel = graph_bounds;

        let padding = TEXT_PADDING * graph_zoom; // pad between any text and panel edge
        let nps_text_width = (peak_text_logical_width * nps_text_zoom).clamp(0.0, panel.width);
        let nps_text_bounds = WidgetBounds::new(
            panel.width - nps_text_width - padding,
            step_header_y - panel.y,
            nps_text_width,
            0.0,
        );

        // The artist background width is the space left for the step artist elements,
        // calculated from the left edge of the panel to the left edge of the NPS text, minus padding.
        let artist_background_width = (nps_text_bounds.x - padding - graph_bounds.x).max(0.0);

        let artist_max_width =
            (artist_background_width - offsets.artist_x - padding).max(0.0) * graph_zoom;

        Self {
            step_header_y,
            artist_background_width,
            artist_x_offset: offsets.artist_x,
            artist_max_width,
            nps_text_bounds,
            nps_text_zoom,
            nps_text_max_width: nps_text_width / nps_text_zoom,
        }
    }
}
