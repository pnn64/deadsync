use super::stepartist_peak_nps::{StepArtistBarY, StepArtistPeakNpsLayout};
use super::{WidgetBounds, WidgetLayout};
use crate::ui_layout::UILayoutType;

const P2_GRAPH_CENTER_Y_OFFSET: f32 = 88.0;
const GRAPH_BODY_HEIGHT: f32 = 47.0;
const BREAKDOWN_TEXT_Y_OFFSET: f32 = 8.5;
const BREAKDOWN_TEXT_ZOOM: f32 = 0.8;
const STEPS_DISPLAY_LIST_WIDTH: f32 = 32.0;
const STEPS_DISPLAY_LIST_HEIGHT: f32 = 152.0;
const STEPS_DISPLAY_LIST_CENTER_Y_OFFSET: f32 = 67.0;
const STEPS_DISPLAY_SLOT_COUNT: usize = 5;
const STEPS_DISPLAY_SLOT_SIZE: f32 = 28.0;
const STEPS_DISPLAY_SLOT_SPACING: f32 = 30.0;
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
    default: DensityGraphOffsets { graph_center: 23.0 },
    expanded: DensityGraphOffsets { graph_center: 62.0 },
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartPanelLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
    pub steps_display_list: StepsDisplayListLayout,
}

impl ChartPanelLayout {
    pub(crate) fn new(
        left_pane: WidgetLayout,
        layout_type: UILayoutType,
        screen_height: f32,
    ) -> Self {
        let right_inset = RIGHT_INSETS.get(layout_type);
        Self {
            bounds: WidgetBounds::new(
                left_pane.bounds.x,
                0.0,
                left_pane.bounds.width - right_inset,
                64.0 * left_pane.zoom,
            ),
            zoom: left_pane.zoom,
            steps_display_list: StepsDisplayListLayout::new(left_pane, screen_height),
        }
    }

    pub(crate) fn compose(
        self,
        screen_height: f32,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> [DensityGraphLayout; 2] {
        let offsets = GRAPH_OFFSETS.get(mode);
        [
            DensityGraphLayout::new(
                self,
                screen_height,
                offsets,
                0.0,
                mode,
                peak_text_logical_width,
            ),
            DensityGraphLayout::new(
                self,
                screen_height,
                offsets,
                P2_GRAPH_CENTER_Y_OFFSET,
                mode,
                peak_text_logical_width,
            ),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StepsDisplayListLayout {
    pub bounds: WidgetBounds,
    pub slot_centers_y: [f32; STEPS_DISPLAY_SLOT_COUNT],
    pub slot_size: f32,
}

impl StepsDisplayListLayout {
    fn new(left_pane: WidgetLayout, screen_height: f32) -> Self {
        let center_x = left_pane.bounds.right() - STEPS_DISPLAY_LIST_WIDTH * 0.5;
        let center_y = screen_height * 0.5 + STEPS_DISPLAY_LIST_CENTER_Y_OFFSET;
        let slot_centers_y = std::array::from_fn(|slot| {
            center_y
                + (slot as f32 - (STEPS_DISPLAY_SLOT_COUNT - 1) as f32 * 0.5)
                    * STEPS_DISPLAY_SLOT_SPACING
        });

        Self {
            bounds: WidgetBounds::new(
                center_x - STEPS_DISPLAY_LIST_WIDTH * 0.5,
                center_y - STEPS_DISPLAY_LIST_HEIGHT * 0.5,
                STEPS_DISPLAY_LIST_WIDTH,
                STEPS_DISPLAY_LIST_HEIGHT,
            ),
            slot_centers_y,
            slot_size: STEPS_DISPLAY_SLOT_SIZE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DensityGraphLayout {
    pub bounds: WidgetBounds,
    pub zoom: f32,
    pub body_height: f32,
    pub breakdown_text_y: f32,
    pub breakdown_text_zoom: f32,
    pub breakdown_text_max_width: f32,
    pub step_artist_peak_nps: StepArtistPeakNpsLayout,
}

impl DensityGraphLayout {
    fn new(
        chart_panel: ChartPanelLayout,
        screen_height: f32,
        offsets: DensityGraphOffsets,
        player_y_offset: f32,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> Self {
        let graph_center_y = screen_height * 0.5 + offsets.graph_center + player_y_offset;
        let bounds = WidgetBounds::new(
            chart_panel.bounds.x,
            graph_center_y - chart_panel.bounds.height * 0.5,
            chart_panel.bounds.width,
            chart_panel.bounds.height,
        );
        let text_zoom = BREAKDOWN_TEXT_ZOOM * chart_panel.zoom;
        let step_artist_peak_nps = StepArtistPeakNpsLayout::for_graph(
            bounds,
            chart_panel.zoom,
            screen_height,
            mode,
            peak_text_logical_width,
        );

        Self {
            bounds,
            zoom: chart_panel.zoom,
            body_height: GRAPH_BODY_HEIGHT * chart_panel.zoom,
            breakdown_text_y: bounds.height - BREAKDOWN_TEXT_Y_OFFSET * chart_panel.zoom,
            breakdown_text_zoom: text_zoom,
            breakdown_text_max_width: bounds.width / text_zoom,
            step_artist_peak_nps,
        }
    }
}
