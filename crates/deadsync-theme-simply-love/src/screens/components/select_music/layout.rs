use crate::ui_layout::UILayoutType;
mod widget;
pub(crate) use widget::{WidgetBounds, WidgetLayout};
mod chart_panel;
pub(crate) use chart_panel::{ChartPanelLayout, DensityGraphLayout};
mod song_banner;
pub(crate) use song_banner::SongBannerLayout;
mod song_details;
pub(crate) use song_details::SongDetailsLayout;
mod stepartist_peak_nps;
pub(crate) use stepartist_peak_nps::{StepArtistBarY, StepArtistPeakNpsLayout};

const LEFT_PANEL_REFERENCE_WIDTH: f32 = 720.0;
const SONG_BANNER_DESIGN_WIDTH: f32 = 418.0;
const SONG_BANNER_DESIGN_HEIGHT: f32 = 164.0;
const SONG_BANNER_CENTER_Y: f32 = 96.0;
const SONG_DETAILS_HEIGHT: f32 = 50.0;
const CHART_PANEL_HEIGHT: f32 = 64.0;
const DIFFICULTY_LIST_WIDTH: f32 = 32.0;
const DIFFICULTY_LIST_HEIGHT: f32 = 152.0;
const DIFFICULTY_LIST_CENTER_Y_OFFSET: f32 = 67.0;
const DIFFICULTY_SLOT_COUNT: usize = 5;
const DIFFICULTY_SLOT_SIZE: f32 = 28.0;

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
    full_left_pane(
        screen_width,
        screen_height,
        (screen_width / LEFT_PANEL_REFERENCE_WIDTH).clamp(0.0, 1.0),
    )
}

// use entire width for wide layouts
fn wide_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    full_left_pane(screen_width, screen_height, 1.0)
}

fn full_left_pane(screen_width: f32, screen_height: f32, zoom: f32) -> WidgetLayout {
    const RIGHT_EDGE_INSET: f32 = 10.0;
    let screen_center_x = screen_width * 0.5;
    let pane_right = screen_center_x - RIGHT_EDGE_INSET;

    WidgetLayout::from_edges(0.0, 0.0, pane_right, screen_height, zoom)
}

// Ultra wide pane uses a fixed width and a center offset to position it correctly.
fn ultra_wide_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    const WIDTH: f32 = 320.0;
    const RIGHT_EDGE_INSET: f32 = 10.0;
    let screen_center_x = screen_width * 0.5;
    let pane_right = screen_center_x - RIGHT_EDGE_INSET;
    let pane_left = pane_right - WIDTH;

    WidgetLayout::from_edges(pane_left, 0.0, pane_right, screen_height, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SongSelectLayout {
    pub zoom: f32,
    pub left_pane: WidgetLayout,
    pub banner: SongBannerLayout,
    pub song_details: SongDetailsLayout,
    pub chart_panel: ChartPanelLayout,
}

impl SongSelectLayout {
    pub fn current() -> Self {
        Self::new(
            deadlib_present::space::screen_width(),
            deadlib_present::space::screen_height(),
            crate::ui_layout::UILayoutType::current(),
        )
    }

    pub fn new(screen_width: f32, screen_height: f32, layout_type: UILayoutType) -> Self {
        let left_pane = (LEFT_PANE_PROFILES.get(layout_type))(screen_width, screen_height);
        let zoom = left_pane.zoom;

        let banner_widget = WidgetLayout::from_center(
            left_pane.bounds.center_x(),
            SONG_BANNER_CENTER_Y,
            left_pane.bounds.width,
            SONG_BANNER_DESIGN_HEIGHT * zoom,
            left_pane.bounds.width / SONG_BANNER_DESIGN_WIDTH,
        );
        let banner = SongBannerLayout::new(banner_widget);

        let song_details_widget = WidgetLayout::from_edges(
            banner.bounds.x,
            banner.bounds.bottom(),
            banner.bounds.right(),
            banner.bounds.bottom() + SONG_DETAILS_HEIGHT,
            left_pane.zoom,
        );
        let song_details = SongDetailsLayout::new(song_details_widget);

        let chart_panel_left = left_pane.bounds.x;
        let chart_panel_right = left_pane.bounds.right() - DIFFICULTY_LIST_WIDTH;
        let chart_panel_height = CHART_PANEL_HEIGHT * zoom;
        let chart_panel_top = (screen_height * 0.5 - chart_panel_height * 0.5)
            .max(song_details.widget.bounds.bottom());
        let chart_panel_widget = WidgetLayout::from_edges(
            chart_panel_left,
            chart_panel_top,
            chart_panel_right,
            chart_panel_top + chart_panel_height,
            zoom,
        );

        let difficulty_list_left = left_pane.bounds.right() - DIFFICULTY_LIST_WIDTH;
        let difficulty_list_right = left_pane.bounds.right();
        let difficulty_list_center_y = screen_height * 0.5 + DIFFICULTY_LIST_CENTER_Y_OFFSET;
        let difficulty_list_top = difficulty_list_center_y - DIFFICULTY_LIST_HEIGHT * 0.5;
        let difficulty_list_widget = WidgetLayout::from_edges(
            difficulty_list_left,
            difficulty_list_top,
            difficulty_list_right,
            difficulty_list_top + DIFFICULTY_LIST_HEIGHT,
            zoom,
        );
        let chart_panel =
            ChartPanelLayout::new(chart_panel_widget, difficulty_list_widget, screen_height);

        Self {
            zoom,
            left_pane,
            banner,
            song_details,
            chart_panel,
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/components/select_music/layout.rs"]
mod tests;
