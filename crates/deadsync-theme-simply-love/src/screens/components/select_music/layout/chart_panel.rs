use super::stepartist_peak_nps::{StepArtistBarY, StepArtistPeakNpsLayout};
use super::{WidgetBounds, WidgetLayout};
use crate::ui_layout::UILayoutType;

const P2_GRAPH_CENTER_Y_OFFSET: f32 = 88.0;
const GRAPH_BODY_HEIGHT: f32 = 47.0;
const BREAKDOWN_TEXT_Y_OFFSET: f32 = 8.5;
const BREAKDOWN_TEXT_ZOOM: f32 = 0.8;
const STANDARD_RIGHT_INSET: f32 = 32.0;
const ULTRA_WIDE_RIGHT_INSET: f32 = 34.0;

#[derive(Clone, Copy)]
struct ChartPanelRightInsets {
    squareish: f32,
    wide: f32,
    ultra_wide: f32,
}

impl ChartPanelRightInsets {
    fn get(self, layout_type: UILayoutType) -> f32 {
        match layout_type {
            UILayoutType::Squareish => self.squareish,
            UILayoutType::Wide => self.wide,
            UILayoutType::UltraWide => self.ultra_wide,
        }
    }
}

const RIGHT_INSETS: ChartPanelRightInsets = ChartPanelRightInsets {
    squareish: STANDARD_RIGHT_INSET,
    wide: STANDARD_RIGHT_INSET,
    ultra_wide: ULTRA_WIDE_RIGHT_INSET,
};

#[derive(Clone, Copy)]
struct DensityGraphOffsets {
    graph_center: f32,
    step_artist_header_gap: f32,
    step_artist_header_screen_height_scale: f32,
    step_artist_x: f32,
}

#[derive(Clone, Copy)]
struct DensityGraphOffsetsByMode {
    default: DensityGraphOffsets,
    expanded: DensityGraphOffsets,
}

impl DensityGraphOffsetsByMode {
    fn get(self, mode: StepArtistBarY) -> DensityGraphOffsets {
        match mode {
            StepArtistBarY::Default => self.default,
            StepArtistBarY::Expanded => self.expanded,
        }
    }
}

const GRAPH_OFFSETS: DensityGraphOffsetsByMode = DensityGraphOffsetsByMode {
    default: DensityGraphOffsets {
        graph_center: 23.0,
        step_artist_header_gap: 32.0,
        step_artist_header_screen_height_scale: 1.0 / 56.0,
        step_artist_x: 49.5,
    },
    expanded: DensityGraphOffsets {
        graph_center: 62.0,
        step_artist_header_gap: 80.0,
        step_artist_header_screen_height_scale: 0.0,
        step_artist_x: 45.0,
    },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartPanelLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
}

impl ChartPanelLayout {
    pub(crate) fn new(left_pane: WidgetLayout, layout_type: UILayoutType) -> Self {
        let right_inset = RIGHT_INSETS.get(layout_type);
        Self {
            bounds: WidgetBounds::new(
                left_pane.bounds.x,
                0.0,
                left_pane.bounds.width - right_inset,
                64.0 * left_pane.zoom,
            ),
            zoom: left_pane.zoom,
        }
    }

    pub(crate) fn compose(
        self,
        screen_height: f32,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> ChartPanelWidgets {
        let offsets = GRAPH_OFFSETS.get(mode);
        let p1_graph = DensityGraphLayout::new(self, screen_height, offsets, 0.0);
        let p2_graph =
            DensityGraphLayout::new(self, screen_height, offsets, P2_GRAPH_CENTER_Y_OFFSET);
        let density_graphs = [p1_graph, p2_graph];
        let step_artist_peak_nps = density_graphs
            .map(|graph| StepArtistPeakNpsLayout::for_graph(graph, peak_text_logical_width));

        ChartPanelWidgets {
            density_graphs,
            step_artist_peak_nps,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartPanelWidgets {
    pub density_graphs: [DensityGraphLayout; 2],
    pub step_artist_peak_nps: [StepArtistPeakNpsLayout; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DensityGraphLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
    pub body_height: f32,
    pub breakdown_text_y: f32,
    pub breakdown_text_zoom: f32,
    pub breakdown_text_max_width: f32,
    pub(super) step_artist_header_y: f32,
    pub(super) artist_x_offset: f32,
}

impl DensityGraphLayout {
    fn new(
        chart_panel: ChartPanelLayout,
        screen_height: f32,
        offsets: DensityGraphOffsets,
        player_y_offset: f32,
    ) -> Self {
        let graph_center_y = screen_height * 0.5 + offsets.graph_center + player_y_offset;
        let bounds = WidgetBounds::new(
            chart_panel.bounds.x,
            graph_center_y - chart_panel.bounds.height * 0.5,
            chart_panel.bounds.width,
            chart_panel.bounds.height,
        );
        let text_zoom = BREAKDOWN_TEXT_ZOOM * chart_panel.zoom;

        Self {
            bounds,
            zoom: chart_panel.zoom,
            body_height: GRAPH_BODY_HEIGHT * chart_panel.zoom,
            breakdown_text_y: bounds.height - BREAKDOWN_TEXT_Y_OFFSET * chart_panel.zoom,
            breakdown_text_zoom: text_zoom,
            breakdown_text_max_width: bounds.width / text_zoom,
            step_artist_header_y: graph_center_y
                - offsets.step_artist_header_gap
                - screen_height * offsets.step_artist_header_screen_height_scale,
            artist_x_offset: offsets.step_artist_x,
        }
    }
}
