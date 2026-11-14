use crate::screens::components::shared::screen_bar::SCREEN_BAR_HEIGHT;
use crate::ui_layout::UILayoutType;
use crate::views::{SelectMusicPresentationPolicyView, SelectMusicSessionView};
mod widget;
pub(crate) use widget::{WidgetBounds, WidgetLayout};
mod difficulty_list;
pub(crate) use difficulty_list::DifficultyListLayout;
mod chart_panel;
pub(crate) use chart_panel::ChartPanelLayout;
pub(crate) use chart_panel::DensityGraphLayout;
mod song_banner;
pub(crate) use song_banner::SongBannerLayout;
mod song_details;
pub(crate) use song_details::SongDetailsLayout;
mod stepartist_peak_nps;
pub(crate) use super::step_artist_bar::{StepArtistBarLayout, StepArtistBarLayoutType};
pub(crate) use stepartist_peak_nps::StepArtistPeakNpsLayout;

const LEFT_PANEL_REFERENCE_WIDTH: f32 = 720.0;
const LEFT_PANEL_RIGHT_EDGE_INSET: f32 = 10.0;
const SONG_BANNER_DESIGN_WIDTH: f32 = 418.0;
const SONG_BANNER_DESIGN_HEIGHT: f32 = 164.0;
const SONG_DETAILS_HEIGHT: f32 = 50.0;
const SECTION_GAP: f32 = 2.0;

type LeftPaneLayoutFn = fn(screen_width: f32, screen_height: f32) -> WidgetLayout;

#[derive(Clone, Copy)]
struct LeftPaneProfiles {
    squareish: LeftPaneLayoutFn,
    wide: LeftPaneLayoutFn,
    ultra_wide: LeftPaneLayoutFn,
}

impl LeftPaneProfiles {
    fn get(self, layout_type: UILayoutType) -> LeftPaneLayoutFn {
        match layout_type {
            UILayoutType::Squareish => self.squareish,
            UILayoutType::Wide => self.wide,
            UILayoutType::UltraWide => self.ultra_wide,
        }
    }
}

const LEFT_PANE_PROFILES: LeftPaneProfiles = LeftPaneProfiles {
    squareish: squareish_left_pane,
    wide: wide_left_pane,
    ultra_wide: ultra_wide_left_pane,
};

// squish ui to fit within the reference width for squareish layouts
fn squareish_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    let screen_center_x = screen_width * 0.5;
    let pane_right = screen_center_x - LEFT_PANEL_RIGHT_EDGE_INSET;
    // use entire left side of screen, and zoom to fill the entire space
    let zoom = (screen_width / LEFT_PANEL_REFERENCE_WIDTH).clamp(0.0, 1.0);
    WidgetLayout::from_edges(0.0, 0.0, pane_right, screen_height, zoom)
}

// use entire width for wide layouts
fn wide_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    const WIDTH: f32 = 320.0;
    let screen_center_x = screen_width * 0.5;
    let pane_right = screen_center_x - LEFT_PANEL_RIGHT_EDGE_INSET;
    let pane_left = pane_right - WIDTH;
    WidgetLayout::from_edges(pane_left, 0.0, pane_right, screen_height, 1.0)
}

// Ultra wide pane uses a fixed width and a center offset to position it correctly.
fn ultra_wide_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    const WIDTH: f32 = 320.0;
    let screen_center_x = screen_width * 0.5;
    let pane_right = screen_center_x - LEFT_PANEL_RIGHT_EDGE_INSET;
    let pane_left = pane_right - WIDTH;

    WidgetLayout::from_edges(pane_left, 0.0, pane_right, screen_height, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongSelectLayout {
    pub(crate) zoom: f32,
    pub(crate) step_artist_expanded: bool,
    pub(crate) left_pane: WidgetLayout,
    pub(crate) banner: SongBannerLayout,
    pub(crate) song_details: SongDetailsLayout,
    pub(crate) step_artist_header: WidgetBounds,
    pub(crate) chart_panel: WidgetBounds,
    pub(crate) density_panel: WidgetLayout,
    pub(crate) pattern_panels: [WidgetLayout; 2],
    pub(crate) difficulty_list: DifficultyListLayout,
    chart_panel_layout: ChartPanelLayout,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TechInfoLayout {
    pub value_columns_x: [f32; 2],
    pub label_columns_x: [f32; 2],
    pub row_centers_y: [f32; 3],
    pub value_zoom: f32,
    pub label_zoom: f32,
    pub stream_max_width: f32,
}

// todo: if we remove the expanded artist view,
// we can simplify the layout calculations and remove the step_artist_expanded field.
impl SongSelectLayout {
    pub fn current(
        presentation: SelectMusicPresentationPolicyView,
        session: SelectMusicSessionView,
    ) -> Self {
        Self::new_with_context(
            deadlib_present::space::screen_width(),
            deadlib_present::space::screen_height(),
            crate::ui_layout::UILayoutType::current(),
            presentation,
            session,
        )
    }

    pub fn density_panel_size(self) -> [f32; 2] {
        [
            self.density_panel.bounds.width,
            self.density_panel.bounds.height,
        ]
    }

    #[cfg(test)]
    pub fn new(screen_width: f32, screen_height: f32, layout_type: UILayoutType) -> Self {
        Self::new_with_context(
            screen_width,
            screen_height,
            layout_type,
            SelectMusicPresentationPolicyView::default(),
            SelectMusicSessionView::default(),
        )
    }

    pub fn new_with_context(
        screen_width: f32,
        screen_height: f32,
        layout_type: UILayoutType,
        presentation: SelectMusicPresentationPolicyView,
        session: SelectMusicSessionView,
    ) -> Self {
        let left_pane = (LEFT_PANE_PROFILES.get(layout_type))(screen_width, screen_height);
        let zoom = left_pane.zoom;
        let banner_zoom = left_pane.bounds.width / SONG_BANNER_DESIGN_WIDTH;

        let banner_widget = WidgetLayout::from_edges(
            left_pane.bounds.x,
            SCREEN_BAR_HEIGHT,
            left_pane.bounds.right(),
            SCREEN_BAR_HEIGHT + SONG_BANNER_DESIGN_HEIGHT * banner_zoom,
            banner_zoom,
        );
        let banner = SongBannerLayout::new(banner_widget);

        let song_details_widget = WidgetLayout::from_edges(
            banner.bounds.x,
            banner.bounds.bottom() + SECTION_GAP,
            banner.bounds.right(),
            banner.bounds.bottom() + SECTION_GAP + SONG_DETAILS_HEIGHT,
            left_pane.zoom,
        );
        let song_details = SongDetailsLayout::new(song_details_widget);

        let chart_panel_layout = ChartPanelLayout::new(
            left_pane.bounds.x,
            left_pane.bounds.right(),
            song_details.widget.bounds.bottom(),
            zoom,
            screen_height,
            presentation,
            session,
        );

        Self {
            zoom,
            step_artist_expanded: chart_panel_layout.step_artist_expanded,
            left_pane,
            banner,
            song_details,
            step_artist_header: chart_panel_layout.step_artist_header,
            chart_panel: chart_panel_layout.bounds,
            density_panel: chart_panel_layout.density_panel,
            pattern_panels: chart_panel_layout.pattern_panels,
            difficulty_list: DifficultyListLayout::anchored_bottom_right(chart_panel_layout.bounds),
            chart_panel_layout,
        }
    }

    pub(crate) fn density_graphs(
        self,
        mode: StepArtistBarLayoutType,
        peak_text_logical_width: f32,
    ) -> [DensityGraphLayout; 2] {
        self.chart_panel_layout
            .compose(mode, peak_text_logical_width)
    }

    pub(crate) fn tech_info(self, panel: WidgetBounds) -> TechInfoLayout {
        let zoom = self.zoom;
        let (value_inset, label_inset, column_spacing, value_zoom, label_zoom, row_step) =
            if self.step_artist_expanded {
                (40.0, 50.0, 150.0, 0.7, 0.8, 17.0)
            } else {
                (39.0, 48.0, 148.0, 0.78, 0.78, 19.0)
            };
        let value_zoom = value_zoom * zoom;
        let column_spacing = column_spacing * zoom;
        let first_value_x = panel.x + value_inset * zoom;
        let first_label_x = panel.x + label_inset * zoom;
        let first_row_y = panel.center_y() - 18.0 * zoom;

        TechInfoLayout {
            value_columns_x: [first_value_x, first_value_x + column_spacing],
            label_columns_x: [first_label_x, first_label_x + column_spacing],
            row_centers_y: std::array::from_fn(|row| first_row_y + row as f32 * row_step * zoom),
            value_zoom,
            label_zoom: label_zoom * zoom,
            stream_max_width: ((column_spacing - 56.0 * zoom) / value_zoom).clamp(58.0, 100.0),
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/components/select_music/layout.rs"]
mod tests;
