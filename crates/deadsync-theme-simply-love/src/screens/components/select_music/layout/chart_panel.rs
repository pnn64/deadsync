use super::super::step_artist_bar::StepArtistBarLayoutType;
use super::{StepArtistPeakNpsLayout, WidgetBounds, WidgetLayout};
use crate::views::{SelectMusicPresentationPolicyView, SelectMusicSessionView};

pub(crate) const DIFFICULTY_LIST_WIDTH: f32 = 32.0;
pub(crate) const DIFFICULTY_SLOT_COUNT: usize = 5;
const DIFFICULTY_SLOT_SIZE: f32 = 28.0;
const DIFFICULTY_LIST_GAP: f32 = 2.0;
const DIFFICULTY_LIST_HEIGHT: f32 =
    DIFFICULTY_SLOT_SIZE * DIFFICULTY_SLOT_COUNT as f32 + DIFFICULTY_LIST_GAP * 7.0;

const CHART_PANEL_HEIGHT: f32 = 64.0;
const CHART_PANEL_TOP_GAP: f32 = 2.0;
const TECH_PANEL_GAP: f32 = 24.0;
const EXPANDED_TECH_PANEL_HEIGHT: f32 = 56.0;
const GRAPH_BODY_HEIGHT: f32 = 47.0;
const BREAKDOWN_TEXT_Y_OFFSET: f32 = 8.5;
const BREAKDOWN_TEXT_ZOOM: f32 = 0.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartPanelLayout {
    pub step_artist_expanded: bool,
    pub step_artist_header: WidgetBounds,
    pub bounds: WidgetBounds,
    pub density_panel: WidgetLayout,
    pub pattern_panels: [WidgetLayout; 2],
    pub zoom: f32,
    pub difficulty_list: DifficultyListLayout,
}

// contains the step artist header, density panel, tech info panels, and difficulty list
impl ChartPanelLayout {
    pub(crate) fn uses_expanded_step_artist(
        presentation: SelectMusicPresentationPolicyView,
        session: SelectMusicSessionView,
    ) -> bool {
        presentation.step_artist_expanded && !session.play_style.is_versus()
    }

    pub(crate) fn new(
        left: f32,
        right: f32,
        previous_bottom: f32,
        zoom: f32,
        screen_height: f32,
        presentation: SelectMusicPresentationPolicyView,
        session: SelectMusicSessionView,
    ) -> Self {
        let step_artist_bar_layout = StepArtistBarLayoutType::from_bool(
            Self::uses_expanded_step_artist(presentation, session),
        );
        let step_artist_expanded =
            matches!(step_artist_bar_layout, StepArtistBarLayoutType::Expanded);
        let density_height = CHART_PANEL_HEIGHT * zoom;
        let step_artist_height = screen_height / step_artist_bar_layout.height_screen_divisor();
        let step_artist_header = WidgetBounds::new(
            left,
            previous_bottom + CHART_PANEL_TOP_GAP,
            right - left,
            step_artist_height,
        );
        let chart_panel_top = step_artist_header.bottom();
        let bounds = WidgetBounds::new(left, chart_panel_top, right - left, DIFFICULTY_LIST_HEIGHT);
        let difficulty_list_left = right - DIFFICULTY_LIST_WIDTH;
        let component_right = difficulty_list_left - DIFFICULTY_LIST_GAP;
        let density_panel = WidgetLayout::from_edges(
            left,
            chart_panel_top,
            component_right,
            chart_panel_top + density_height,
            zoom,
        );
        let difficulty_list_widget =
            WidgetLayout::from_edges(difficulty_list_left, bounds.y, right, bounds.bottom(), zoom);

        let lower_pattern_gap = if step_artist_expanded {
            0.0 // vertical space hogged by expanded step artist bar
        } else {
            TECH_PANEL_GAP
        };
        let lower_pattern_top = density_panel.bounds.bottom() + lower_pattern_gap;
        let lower_pattern_bottom = if step_artist_expanded {
            lower_pattern_top + EXPANDED_TECH_PANEL_HEIGHT * zoom
        } else {
            bounds.bottom()
        };
        let pattern_panels = [
            density_panel,
            WidgetLayout::from_edges(
                left,
                lower_pattern_top,
                component_right,
                lower_pattern_bottom,
                zoom,
            ),
        ];

        Self {
            step_artist_expanded,
            step_artist_header,
            bounds,
            density_panel,
            pattern_panels,
            zoom,
            difficulty_list: DifficultyListLayout::new(difficulty_list_widget),
        }
    }

    pub(crate) fn compose(
        self,
        mode: StepArtistBarLayoutType,
        peak_text_logical_width: f32,
    ) -> [DensityGraphLayout; 2] {
        let panel_center_y = self.density_panel.bounds.center_y();
        let p2_graph_center_y = self.bounds.bottom() - self.density_panel.bounds.height * 0.5;
        [
            DensityGraphLayout::new(
                self,
                panel_center_y,
                mode,
                peak_text_logical_width,
                self.step_artist_header,
            ),
            DensityGraphLayout::new(
                self,
                p2_graph_center_y,
                mode,
                peak_text_logical_width,
                self.step_artist_header,
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
        const BORDER: f32 = 2.0;
        let slot_height = (widget.bounds.height - BORDER * 2.0) / DIFFICULTY_SLOT_COUNT as f32;
        let slot_centers_y = std::array::from_fn(|slot| {
            widget.bounds.y + BORDER + (slot as f32 + 0.5) * slot_height
        });

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
    pub step_artist_bar: super::StepArtistBarLayout,
    pub step_artist_peak_nps: StepArtistPeakNpsLayout,
}

impl DensityGraphLayout {
    fn new(
        chart_panel: ChartPanelLayout,
        panel_center_y: f32,
        mode: StepArtistBarLayoutType,
        peak_text_logical_width: f32,
        step_artist_header: WidgetBounds,
    ) -> Self {
        let graph_center_y = panel_center_y;
        let density_bounds = chart_panel.density_panel.bounds;
        let bounds = WidgetBounds::new(
            density_bounds.x,
            graph_center_y - density_bounds.height * 0.5,
            density_bounds.width,
            density_bounds.height,
        );
        let text_zoom = BREAKDOWN_TEXT_ZOOM * chart_panel.zoom;
        let (step_artist_bar, step_artist_peak_nps) = StepArtistPeakNpsLayout::for_graph(
            bounds,
            chart_panel.zoom,
            mode,
            peak_text_logical_width,
            step_artist_header,
        );

        Self {
            bounds,
            zoom: chart_panel.zoom,
            body_height: GRAPH_BODY_HEIGHT * chart_panel.zoom,
            breakdown_text_y: bounds.height - BREAKDOWN_TEXT_Y_OFFSET * chart_panel.zoom,
            breakdown_text_zoom: text_zoom,
            breakdown_text_max_width: bounds.width / text_zoom,
            step_artist_bar,
            step_artist_peak_nps,
        }
    }
}
