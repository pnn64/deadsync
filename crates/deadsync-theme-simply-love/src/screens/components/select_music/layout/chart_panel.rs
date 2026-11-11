use super::stepartist_peak_nps::{StepArtistBarY, StepArtistPeakNpsLayout};
use super::{DIFFICULTY_SLOT_COUNT, DIFFICULTY_SLOT_SIZE, WidgetBounds, WidgetLayout};

const P2_GRAPH_CENTER_Y_OFFSET: f32 = 88.0;
const GRAPH_BODY_HEIGHT: f32 = 47.0;
const BREAKDOWN_TEXT_Y_OFFSET: f32 = 8.5;
const BREAKDOWN_TEXT_ZOOM: f32 = 0.8;
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
    pub widget: WidgetLayout,
    pub bounds: WidgetBounds,
    pub zoom: f32,
    pub difficulty_list: DifficultyListLayout,
    screen_height: f32,
}

impl ChartPanelLayout {
    pub(crate) fn new(
        widget: WidgetLayout,
        difficulty_list_widget: WidgetLayout,
        screen_height: f32,
    ) -> Self {
        Self {
            widget,
            bounds: widget.bounds,
            zoom: widget.zoom,
            difficulty_list: DifficultyListLayout::new(difficulty_list_widget),
            screen_height,
        }
    }

    pub(crate) fn compose(
        self,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> [DensityGraphLayout; 2] {
        let offsets = GRAPH_OFFSETS.get(mode);
        let panel_center_y = self.widget.bounds.center_y();
        [
            DensityGraphLayout::new(
                self,
                panel_center_y,
                self.screen_height,
                offsets,
                0.0,
                mode,
                peak_text_logical_width,
            ),
            DensityGraphLayout::new(
                self,
                panel_center_y,
                self.screen_height,
                offsets,
                P2_GRAPH_CENTER_Y_OFFSET,
                mode,
                peak_text_logical_width,
            ),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DifficultyListLayout {
    pub bounds: WidgetBounds,
    pub slot_centers_y: [f32; DIFFICULTY_SLOT_COUNT],
    pub slot_size: f32,
}

impl DifficultyListLayout {
    fn new(widget: WidgetLayout) -> Self {
        let slot_height = widget.bounds.height / DIFFICULTY_SLOT_COUNT as f32;
        let slot_centers_y =
            std::array::from_fn(|slot| widget.bounds.y + (slot as f32 + 0.5) * slot_height);

        Self {
            bounds: widget.bounds,
            slot_centers_y,
            slot_size: DIFFICULTY_SLOT_SIZE,
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
        panel_center_y: f32,
        screen_height: f32,
        offsets: DensityGraphOffsets,
        player_y_offset: f32,
        mode: StepArtistBarY,
        peak_text_logical_width: f32,
    ) -> Self {
        let graph_center_y = panel_center_y + offsets.graph_center + player_y_offset;
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
