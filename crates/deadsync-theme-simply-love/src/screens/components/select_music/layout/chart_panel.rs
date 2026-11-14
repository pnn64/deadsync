use super::super::step_artist_bar::StepArtistBarLayoutType;
use super::difficulty_list::{
    DIFFICULTY_LIST_HEIGHT, DIFFICULTY_LIST_WIDTH, DIFFICULTY_SLOT_BORDER,
};
use super::{StepArtistPeakNpsLayout, WidgetBounds, WidgetLayout};
use crate::views::{SelectMusicPresentationPolicyView, SelectMusicSessionView};

const CHART_PANEL_HEIGHT: f32 = 64.0;
const CHART_PANEL_TOP_GAP: f32 = 2.0;
const TECH_PANEL_GAP: f32 = 24.0;
const EXPANDED_TECH_PANEL_HEIGHT: f32 = 56.0;
const GRAPH_BODY_HEIGHT: f32 = 47.0;
const BREAKDOWN_TEXT_Y_OFFSET: f32 = 8.5;
const BREAKDOWN_TEXT_ZOOM: f32 = 0.8;

fn get_panel_heights(step_artist_expanded: bool, is_p2_single: bool, zoom: f32) -> (f32, f32, f32) {
    let density_height = CHART_PANEL_HEIGHT * zoom;
    let expanded_tech_height = EXPANDED_TECH_PANEL_HEIGHT * zoom;
    let top_height = if step_artist_expanded && is_p2_single {
        expanded_tech_height
    } else {
        density_height
    };
    let gap = if step_artist_expanded {
        2.0
    } else {
        TECH_PANEL_GAP
    };
    let bottom_height = if step_artist_expanded && !is_p2_single {
        expanded_tech_height
    } else if step_artist_expanded {
        density_height
    } else {
        DIFFICULTY_LIST_HEIGHT - top_height - gap
    };

    (top_height, gap, bottom_height)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartPanelLayout {
    pub step_artist_expanded: bool,
    pub step_artist_header: WidgetBounds,
    pub bounds: WidgetBounds,
    pub density_panel: WidgetLayout,
    pub pattern_panels: [WidgetLayout; 2],
    pub zoom: f32,
}

// contains the step artist header, density panel, tech info panels
impl ChartPanelLayout {
    pub(crate) fn uses_expanded_step_artist(
        presentation: SelectMusicPresentationPolicyView,
        session: SelectMusicSessionView,
    ) -> bool {
        presentation.step_artist_expanded && !session.play_style.is_versus()
    }

    pub(crate) fn centered_between(mut self, top: f32, bottom: f32) -> Self {
        let offset = ((bottom - top - self.bounds.height) * 0.5).max(0.0);
        self.step_artist_header = self.step_artist_header.translated_y(offset);
        self.bounds = self.bounds.translated_y(offset);
        self.density_panel = self.density_panel.translated_y(offset);
        self.pattern_panels = self.pattern_panels.map(|panel| panel.translated_y(offset));
        self
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
        let is_p2_single =
            deadsync_profile::is_single_p2_side(session.play_style, session.player_side);
        let density_height = CHART_PANEL_HEIGHT * zoom;
        let (top_pattern_height, lower_pattern_gap, bottom_pattern_height) =
            get_panel_heights(step_artist_expanded, is_p2_single, zoom);
        let chart_panel_height = top_pattern_height + lower_pattern_gap + bottom_pattern_height;
        let step_artist_height = screen_height / step_artist_bar_layout.height_screen_divisor();
        let step_artist_header = WidgetBounds::new(
            left,
            previous_bottom + CHART_PANEL_TOP_GAP,
            right - left,
            step_artist_height,
        );
        let chart_panel_top = step_artist_header.bottom();
        let bounds = WidgetBounds::new(left, chart_panel_top, right - left, chart_panel_height);
        let difficulty_list_left = right - DIFFICULTY_LIST_WIDTH;
        let component_right = difficulty_list_left - DIFFICULTY_SLOT_BORDER;
        let density_panel_top = if is_p2_single {
            bounds.bottom() - density_height
        } else {
            chart_panel_top
        };
        let density_panel = WidgetLayout::from_edges(
            left,
            density_panel_top,
            component_right,
            density_panel_top + density_height,
            zoom,
        );

        let lower_pattern_top = chart_panel_top + top_pattern_height + lower_pattern_gap;
        let lower_pattern_bottom = lower_pattern_top + bottom_pattern_height;
        let pattern_panels = [
            WidgetLayout::from_edges(
                left,
                chart_panel_top,
                component_right,
                chart_panel_top + top_pattern_height,
                zoom,
            ),
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
