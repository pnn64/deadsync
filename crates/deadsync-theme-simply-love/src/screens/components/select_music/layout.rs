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
    let pane_width = screen_center_x - RIGHT_EDGE_INSET;
    let pane_centre_x = screen_center_x - pane_width * 0.5 - RIGHT_EDGE_INSET;

    WidgetLayout::from_center(
        pane_centre_x,
        screen_height * 0.5,
        pane_width,
        screen_height,
        zoom,
    )
}

// Ultra wide pane uses a fixed width and a center offset to position it correctly.
fn ultra_wide_left_pane(screen_width: f32, screen_height: f32) -> WidgetLayout {
    const WIDTH: f32 = 320.0;
    const RIGHT_EDGE_INSET: f32 = 10.0;
    let screen_center_x = screen_width * 0.5;
    let center_x = screen_center_x - WIDTH * 0.5 - RIGHT_EDGE_INSET;

    WidgetLayout::from_center(center_x, screen_height * 0.5, WIDTH, screen_height, 1.0)
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
        let banner = SongBannerLayout::new(left_pane);
        let song_details = SongDetailsLayout::new(left_pane, screen_height);
        let chart_panel = ChartPanelLayout::new(left_pane, layout_type, screen_height);

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
