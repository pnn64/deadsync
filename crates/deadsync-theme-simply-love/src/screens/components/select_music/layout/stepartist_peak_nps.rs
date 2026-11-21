use super::super::step_artist_bar::{StepArtistBarLayout, StepArtistBarLayoutType};
use super::WidgetBounds;

// We don't want any text to touch the edges of any panel
const TEXT_PADDING: f32 = 8.0;

#[derive(Clone, Copy)]
struct StepArtistPeakNpsOffsets {
    artist_x: f32,
}

#[derive(Clone, Copy)]
struct StepArtistPeakNpsOffsetsByMode {
    default: StepArtistPeakNpsOffsets,
    expanded: StepArtistPeakNpsOffsets,
}

impl StepArtistPeakNpsOffsetsByMode {
    fn get(self, mode: StepArtistBarLayoutType) -> StepArtistPeakNpsOffsets {
        match mode {
            StepArtistBarLayoutType::Legacy => self.default,
            StepArtistBarLayoutType::Expanded => self.expanded,
        }
    }
}

const OFFSETS: StepArtistPeakNpsOffsetsByMode = StepArtistPeakNpsOffsetsByMode {
    default: StepArtistPeakNpsOffsets { artist_x: 49.5 },
    expanded: StepArtistPeakNpsOffsets { artist_x: 45.0 },
};

fn artist_bar_vertical_layout(
    mode: StepArtistBarLayoutType,
    graph_bounds: WidgetBounds,
    step_artist_header: WidgetBounds,
) -> (f32, f32) {
    match mode {
        StepArtistBarLayoutType::Legacy => {
            let step_header_y = graph_bounds.y - step_artist_header.height * 0.5;
            (
                step_header_y,
                step_header_y - step_artist_header.height * 0.5,
            )
        }
        StepArtistBarLayoutType::Expanded => (step_artist_header.center_y(), step_artist_header.y),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepArtistPeakNpsLayout {
    pub step_header_y: f32,
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
        mode: StepArtistBarLayoutType,
        peak_text_logical_width: f32,
        step_artist_header: WidgetBounds,
    ) -> (StepArtistBarLayout, Self) {
        let offsets = OFFSETS.get(mode);
        let (step_header_y, artist_background_y) =
            artist_bar_vertical_layout(mode, graph_bounds, step_artist_header);
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
        let artist_background_width = (nps_text_bounds.x - padding).max(0.0);

        let artist_max_width =
            (artist_background_width - offsets.artist_x - padding).max(0.0) * graph_zoom;

        (
            StepArtistBarLayout::new(
                artist_background_width,
                artist_background_y,
                step_artist_header.height,
                offsets.artist_x,
                artist_max_width,
                match mode {
                    StepArtistBarLayoutType::Legacy => 1,
                    StepArtistBarLayoutType::Expanded => 3,
                },
            ),
            Self {
                step_header_y,
                nps_text_bounds,
                nps_text_zoom,
                nps_text_max_width: nps_text_width / nps_text_zoom,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{StepArtistBarLayoutType, artist_bar_vertical_layout};
    use crate::screens::components::select_music::layout::WidgetBounds;

    #[test]
    fn legacy_vertical_layout_anchors_to_graph_top() {
        let graph = WidgetBounds::new(0.0, 100.0, 200.0, 50.0);
        let header = WidgetBounds::new(0.0, 25.0, 200.0, 30.0);
        let (step_header_y, background_y) =
            artist_bar_vertical_layout(StepArtistBarLayoutType::Legacy, graph, header);
        let expected_header_y = graph.y - header.height * 0.5;

        assert!((step_header_y - expected_header_y).abs() < f32::EPSILON);
        assert!((background_y - (expected_header_y - 15.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn expanded_vertical_layout_follows_header_bounds() {
        let graph = WidgetBounds::new(0.0, 100.0, 200.0, 50.0);
        let header = WidgetBounds::new(0.0, 25.0, 200.0, 80.0);
        let (step_header_y, background_y) =
            artist_bar_vertical_layout(StepArtistBarLayoutType::Expanded, graph, header);

        assert_eq!(step_header_y, 65.0);
        assert_eq!(background_y, 25.0);
    }
}
