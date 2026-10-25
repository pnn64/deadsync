use deadlib_present::space::Metrics;
use deadsync_profile::PlayerSide;
use glam::Mat4;

pub const STEP_STATS_SONG_BANNER_ZOOM: f32 = 0.4;
pub const STEP_STATS_BANNER_W: f32 = 418.0;
pub const STEP_STATS_BANNER_H: f32 = 164.0;
pub const STEP_STATS_PANE_CONTENT_W: f32 = 400.0;
const STEP_STATS_SONG_INFO_PADDING: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsPaneParams {
    pub screen_w: f32,
    pub screen_h: f32,
    pub screen_center_x: f32,
    pub screen_center_y: f32,
    pub playfield_center_x: f32,
    pub player_side: PlayerSide,
    pub num_players: usize,
    pub notefield_width: Option<f32>,
    pub wide: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsPaneLayout {
    pub sidepane_center_x: f32,
    pub sidepane_center_y: f32,
    pub sidepane_width: f32,
    pub note_field_is_centered: bool,
    pub is_ultrawide: bool,
    pub banner_data_zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoubleStepStatsLayout {
    pub pane_center_x: f32,
    pub pane_center_y: f32,
    pub note_field_is_centered: bool,
    pub banner_data_zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CenteredSingleSplitLayout {
    pub pane: DoubleStepStatsLayout,
    pub sidepane_width: f32,
    pub panel_width: f32,
    pub left_panel_center_x: f32,
    pub right_panel_center_x: f32,
    pub graph: StepStatsGraphRect,
}

impl CenteredSingleSplitLayout {
    #[must_use]
    pub fn song_banner(self) -> StepStatsSpritePlacement {
        StepStatsSpritePlacement {
            x: 16.0f32.mul_add(self.pane.banner_data_zoom, self.right_panel_center_x),
            y: 200.0f32.mul_add(-self.pane.banner_data_zoom, self.pane.pane_center_y),
            zoom: STEP_STATS_SONG_BANNER_ZOOM * self.pane.banner_data_zoom,
        }
    }

    #[must_use]
    pub fn pack_banner(self) -> StepStatsSpritePlacement {
        let song_banner = self.song_banner();
        let song_w = STEP_STATS_BANNER_W * song_banner.zoom;
        let pack_w = STEP_STATS_BANNER_W * 0.2 * self.pane.banner_data_zoom;
        StepStatsSpritePlacement {
            x: song_w.mul_add(-0.5, song_banner.x) + pack_w * 0.5,
            y: 20.0f32.mul_add(self.pane.banner_data_zoom, self.pane.pane_center_y),
            zoom: 0.2 * self.pane.banner_data_zoom,
        }
    }

    #[must_use]
    pub fn holds_mines_rolls_frame(self) -> StepStatsFramePlacement {
        StepStatsFramePlacement {
            center_x: 56.0f32.mul_add(self.pane.banner_data_zoom, self.left_panel_center_x),
            center_y: 0.8f32
                .mul_add(28.0, -10.0)
                .mul_add(self.pane.banner_data_zoom, self.pane.pane_center_y),
            zoom: 0.8 * self.pane.banner_data_zoom,
        }
    }

    #[must_use]
    pub fn scorebox_frame(self) -> StepStatsFramePlacement {
        StepStatsFramePlacement {
            center_x: 16.0f32.mul_add(self.pane.banner_data_zoom, self.right_panel_center_x),
            center_y: 115.0f32.mul_add(-self.pane.banner_data_zoom, self.pane.pane_center_y),
            zoom: self.pane.banner_data_zoom,
        }
    }

    #[must_use]
    pub fn judgment_origin_x(self) -> f32 {
        (-81.0f32).mul_add(self.pane.banner_data_zoom, self.left_panel_center_x)
    }

    #[must_use]
    pub fn time_base_x(self) -> f32 {
        (-6.0f32).mul_add(self.pane.banner_data_zoom, self.left_panel_center_x)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsGraphRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsPaneRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[must_use]
pub fn step_stats_aspect_enabled(screen_w: f32, screen_h: f32) -> bool {
    screen_h > 0.0 && screen_w >= screen_h
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsSpritePlacement {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsSongInfoSlide {
    pub left: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub banner_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsSongInfoTextPlacement {
    pub labels_x: f32,
    pub values_x: f32,
    pub values_max_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsFramePlacement {
    pub center_x: f32,
    pub center_y: f32,
    pub zoom: f32,
}

#[must_use]
pub fn pane_layout(params: StepStatsPaneParams) -> StepStatsPaneLayout {
    let sh = params.screen_h.max(1.0);
    let is_ultrawide = params.screen_w / sh > 21.0 / 9.0;
    let note_field_is_centered = (params.playfield_center_x - params.screen_center_x).abs() < 1.0;

    let mut sidepane_width = params.screen_w * 0.5;
    let mut sidepane_center_x = match params.player_side {
        PlayerSide::P1 => params.screen_w * 0.75,
        PlayerSide::P2 => params.screen_w * 0.25,
    };

    if !is_ultrawide && note_field_is_centered && params.wide {
        let nf_width = params.notefield_width.unwrap_or(256.0).max(1.0);
        sidepane_width = ((params.screen_w - nf_width) * 0.5).max(1.0);
        sidepane_center_x = match params.player_side {
            PlayerSide::P1 => {
                (sidepane_width - nf_width).mul_add(0.5, params.screen_center_x + nf_width)
            }
            PlayerSide::P2 => {
                (sidepane_width - nf_width).mul_add(-0.5, params.screen_center_x - nf_width)
            }
        };
    }

    if is_ultrawide && params.num_players > 1 {
        sidepane_width = params.screen_w * 0.2;
        sidepane_center_x = match params.player_side {
            PlayerSide::P1 => sidepane_width * 0.5,
            PlayerSide::P2 => params.screen_w - sidepane_width * 0.5,
        };
    }

    let banner_data_zoom = if note_field_is_centered && params.wide && !is_ultrawide {
        let ar = params.screen_w / sh;
        let t = ((ar - 16.0 / 10.0) / (16.0 / 9.0 - 16.0 / 10.0)).clamp(0.0, 1.0);
        (0.925_f32 - 0.825).mul_add(t, 0.825)
    } else {
        1.0
    };

    StepStatsPaneLayout {
        sidepane_center_x,
        sidepane_center_y: params.screen_center_y + 80.0,
        sidepane_width,
        note_field_is_centered,
        is_ultrawide,
        banner_data_zoom,
    }
}

#[must_use]
pub fn pane_position_scale(layout: StepStatsPaneLayout) -> f32 {
    (layout.sidepane_width / STEP_STATS_PANE_CONTENT_W).min(1.0)
}

#[must_use]
pub fn pane_anchor_x(layout: StepStatsPaneLayout, local_x: f32) -> f32 {
    local_x.mul_add(pane_position_scale(layout), layout.sidepane_center_x)
}

#[must_use]
pub fn pane_view_proj(screen_w: f32, screen_h: f32) -> Mat4 {
    Metrics::centered(screen_w, screen_h).projection()
}

#[must_use]
pub fn pane_rect(layout: StepStatsPaneLayout, screen_h: f32) -> StepStatsPaneRect {
    StepStatsPaneRect {
        left: layout.sidepane_center_x - layout.sidepane_width * 0.5,
        top: 0.0,
        right: layout.sidepane_center_x + layout.sidepane_width * 0.5,
        bottom: screen_h.max(0.0),
    }
}

#[must_use]
pub fn judgment_count_center_x(
    layout: StepStatsPaneLayout,
    player_side: PlayerSide,
    wide: bool,
    num_players: usize,
) -> f32 {
    let side_sign = match player_side {
        PlayerSide::P1 => 1.0,
        PlayerSide::P2 => -1.0,
    };
    let local_x = if layout.is_ultrawide && num_players > 1 {
        154.0 * side_sign
    } else if layout.note_field_is_centered && wide {
        -156.0 * side_sign
    } else {
        -(if wide { 204.0 } else { 152.0 }) * side_sign
    };
    layout.sidepane_center_x + local_x * layout.banner_data_zoom
}

#[must_use]
pub fn song_info_text_zoom(layout: StepStatsPaneLayout) -> f32 {
    let zoom = if layout.note_field_is_centered {
        0.9
    } else {
        0.75
    };
    zoom * layout.banner_data_zoom
}

#[must_use]
pub fn density_graph_width(current_graph_w: f32, sidepane_width: f32, double: bool) -> f32 {
    if current_graph_w > 0.0 {
        return current_graph_w;
    }
    let width = if double {
        sidepane_width * 0.95
    } else {
        sidepane_width.round()
    };
    width.max(1.0)
}

#[must_use]
pub fn density_graph_rect(current_graph_w: f32, layout: StepStatsPaneLayout) -> StepStatsGraphRect {
    let graph_w = density_graph_width(current_graph_w, layout.sidepane_width, false);
    StepStatsGraphRect {
        x: graph_w.mul_add(-0.5, layout.sidepane_center_x),
        y: layout.sidepane_center_y + 55.0,
        w: graph_w,
    }
}

#[must_use]
pub fn song_banner_placement(
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: PlayerSide,
    num_players: usize,
) -> StepStatsSpritePlacement {
    let mut local_x = if layout.note_field_is_centered && wide {
        72.0
    } else {
        70.0
    };
    if player_side == PlayerSide::P2 {
        local_x *= -1.0;
    }
    if layout.is_ultrawide && num_players > 1 {
        local_x *= -1.0;
    }
    StepStatsSpritePlacement {
        x: pane_anchor_x(layout, local_x * layout.banner_data_zoom),
        y: 200.0f32.mul_add(-layout.banner_data_zoom, layout.sidepane_center_y),
        zoom: STEP_STATS_SONG_BANNER_ZOOM * layout.banner_data_zoom,
    }
}

#[must_use]
pub fn song_info_slide(
    layout: StepStatsPaneLayout,
    player_side: PlayerSide,
    wide: bool,
    num_players: usize,
    show_duration: bool,
    live_timing_rows: usize,
    graph_top: f32,
) -> StepStatsSongInfoSlide {
    let pane_left = layout.sidepane_center_x - layout.sidepane_width * 0.5;
    let pane_right = layout.sidepane_center_x + layout.sidepane_width * 0.5;
    let time_anchor = judgment_count_center_x(layout, player_side, wide, num_players);
    let timing_frame = scorebox_frame(layout, wide, player_side, num_players);
    let timing_half_width = crate::scorebox::SCOREBOX_W * timing_frame.zoom * 0.5;
    let left = pane_left.max(time_anchor.min(timing_frame.center_x - timing_half_width));
    let right = pane_right.min(time_anchor.max(timing_frame.center_x + timing_half_width));

    let time_remaining_y = layout.sidepane_center_y - 40.0 * layout.banner_data_zoom - 7.0;
    let time_total_y = layout.sidepane_center_y - 40.0 * layout.banner_data_zoom + 13.0;
    let mut occupied_bottom = if show_duration {
        time_total_y + 10.0
    } else {
        layout.sidepane_center_y
    };

    if live_timing_rows > 0 {
        let compact = live_timing_rows >= 3;
        let row_height = if compact { 20.0 * 0.68 } else { 20.0 };
        let first_row_y = if compact {
            time_remaining_y - row_height * 0.12
        } else {
            time_remaining_y
        };
        let last_row_y = first_row_y + row_height * live_timing_rows.saturating_sub(1) as f32;
        let text_zoom = layout.banner_data_zoom * 0.833 * if compact { 0.82 } else { 1.0 };
        occupied_bottom = occupied_bottom.max(last_row_y + 8.0 * text_zoom);
    }

    let width = (right - left).max(1.0);
    let height = (graph_top - occupied_bottom).max(1.0);
    let pack_banner_zoom = if layout.note_field_is_centered {
        0.2
    } else {
        0.25
    };
    let banner_width = (STEP_STATS_BANNER_W * pack_banner_zoom * layout.banner_data_zoom)
        .min(width)
        .min(height * STEP_STATS_BANNER_W / STEP_STATS_BANNER_H);

    StepStatsSongInfoSlide {
        left,
        center_y: (occupied_bottom + graph_top) * 0.5,
        width,
        height,
        banner_width,
    }
}

#[must_use]
pub fn pack_banner_placement(slide: StepStatsSongInfoSlide) -> StepStatsSpritePlacement {
    StepStatsSpritePlacement {
        x: slide.left + slide.banner_width * 0.5,
        y: slide.center_y,
        zoom: slide.banner_width / STEP_STATS_BANNER_W,
    }
}

#[must_use]
pub fn song_info_text_placement(
    slide: StepStatsSongInfoSlide,
    show_labels: bool,
    text_zoom: f32,
) -> StepStatsSongInfoTextPlacement {
    let text_left = slide.left + slide.banner_width + STEP_STATS_SONG_INFO_PADDING;
    let text_right = slide.left + slide.width;
    let labels_x = text_left;
    let values_x = if show_labels {
        labels_x + 45.0 * text_zoom
    } else {
        labels_x
    };

    StepStatsSongInfoTextPlacement {
        labels_x,
        values_x,
        values_max_width: (text_right - values_x - STEP_STATS_SONG_INFO_PADDING).max(1.0),
    }
}

#[must_use]
pub fn holds_mines_rolls_frame(
    layout: StepStatsPaneLayout,
    player_side: PlayerSide,
) -> StepStatsFramePlacement {
    let local_x = match player_side {
        PlayerSide::P1 => 155.0,
        PlayerSide::P2 => -85.0,
    };
    StepStatsFramePlacement {
        center_x: pane_anchor_x(layout, local_x * layout.banner_data_zoom),
        center_y: 112.0f32.mul_add(-layout.banner_data_zoom, layout.sidepane_center_y),
        zoom: layout.banner_data_zoom,
    }
}

#[must_use]
pub fn scorebox_frame(
    layout: StepStatsPaneLayout,
    wide: bool,
    player_side: PlayerSide,
    num_players: usize,
) -> StepStatsFramePlacement {
    let x_sign = match player_side {
        PlayerSide::P1 => 1.0,
        PlayerSide::P2 => -1.0,
    };
    let mut local_x = 70.0 * x_sign;
    if layout.note_field_is_centered && wide {
        local_x = 2.0f32.mul_add(x_sign, local_x);
    }
    if layout.is_ultrawide && num_players > 1 {
        local_x = -local_x;
    }
    StepStatsFramePlacement {
        center_x: pane_anchor_x(layout, local_x * layout.banner_data_zoom),
        center_y: 115.0f32.mul_add(-layout.banner_data_zoom, layout.sidepane_center_y),
        zoom: layout.banner_data_zoom,
    }
}

#[must_use]
pub fn double_pane_layout(
    screen_center_x: f32,
    screen_center_y: f32,
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
) -> DoubleStepStatsLayout {
    let note_field_is_centered = (playfield_center_x - screen_center_x).abs() < 1.0;
    let banner_data_zoom = if note_field_is_centered {
        let ar = screen_w / screen_h.max(1.0);
        let t = ((ar - 16.0 / 10.0) / (16.0 / 9.0 - 16.0 / 10.0)).clamp(0.0, 1.0);
        (0.925_f32 - 0.825).mul_add(t, 0.825)
    } else {
        1.0
    };

    DoubleStepStatsLayout {
        pane_center_x: screen_center_x,
        pane_center_y: screen_center_y + 80.0,
        note_field_is_centered,
        banner_data_zoom,
    }
}

#[must_use]
pub fn centered_single_split_layout(
    screen_center_x: f32,
    screen_center_y: f32,
    screen_w: f32,
    screen_h: f32,
    playfield_center_x: f32,
    notefield_width: f32,
    current_graph_w: f32,
) -> Option<CenteredSingleSplitLayout> {
    const PANEL_CONTENT_WIDTH: f32 = 200.0;
    const MIN_PANEL_ZOOM: f32 = 0.8;
    const GRAPH_HEIGHT: f32 = 105.0;

    if screen_h <= 0.0 || screen_w < screen_h || screen_w / screen_h >= 1.6 {
        return None;
    }

    let pane = double_pane_layout(
        screen_center_x,
        screen_center_y,
        screen_w,
        screen_h,
        playfield_center_x,
    );
    if !pane.note_field_is_centered {
        return None;
    }

    let sidepane_width = double_sidepane_width(screen_w, notefield_width);
    let panel_zoom = (sidepane_width / PANEL_CONTENT_WIDTH).min(1.0);
    if panel_zoom < MIN_PANEL_ZOOM {
        return None;
    }

    let banner_half_height = STEP_STATS_BANNER_H * STEP_STATS_SONG_BANNER_ZOOM * 0.5;
    let banner_top_y = pane.pane_center_y - 200.0 - banner_half_height;
    let pane = DoubleStepStatsLayout {
        pane_center_y: banner_top_y + (200.0 + banner_half_height) * panel_zoom,
        banner_data_zoom: panel_zoom,
        ..pane
    };
    let panel_width = PANEL_CONTENT_WIDTH * panel_zoom;
    let left_field_edge = screen_center_x - notefield_width * 0.5;
    let right_field_edge = screen_center_x + notefield_width * 0.5;
    let graph = StepStatsGraphRect {
        x: right_field_edge,
        y: screen_h - GRAPH_HEIGHT,
        w: density_graph_width(current_graph_w, sidepane_width, false),
    };

    if graph.w > sidepane_width
        || graph.x + graph.w > screen_w
        || graph.y < 0.0
        || graph.y + GRAPH_HEIGHT > screen_h
    {
        return None;
    }

    Some(CenteredSingleSplitLayout {
        pane,
        sidepane_width,
        panel_width,
        left_panel_center_x: left_field_edge - panel_width * 0.5,
        right_panel_center_x: right_field_edge + panel_width * 0.5,
        graph,
    })
}

#[must_use]
pub fn double_song_banner_placement(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsSpritePlacement {
    StepStatsSpritePlacement {
        x: (notefield_width - 140.0).mul_add(layout.banner_data_zoom, layout.pane_center_x),
        y: 200.0f32.mul_add(-layout.banner_data_zoom, layout.pane_center_y),
        zoom: STEP_STATS_SONG_BANNER_ZOOM * layout.banner_data_zoom,
    }
}

#[must_use]
pub fn double_pack_banner_placement(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsSpritePlacement {
    let final_size = if layout.note_field_is_centered {
        0.2
    } else {
        0.25
    };
    let song_banner = double_song_banner_placement(layout, notefield_width);
    let song_w = STEP_STATS_BANNER_W * song_banner.zoom;
    let pack_w = STEP_STATS_BANNER_W * final_size * layout.banner_data_zoom;

    StepStatsSpritePlacement {
        x: song_w.mul_add(-0.5, song_banner.x) + pack_w * 0.5,
        y: 20.0f32.mul_add(layout.banner_data_zoom, layout.pane_center_y),
        zoom: final_size * layout.banner_data_zoom,
    }
}

#[must_use]
pub fn double_holds_mines_rolls_frame(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsFramePlacement {
    StepStatsFramePlacement {
        center_x: (-notefield_width + 212.0).mul_add(layout.banner_data_zoom, layout.pane_center_x),
        center_y: 0.8f32
            .mul_add(28.0, -10.0)
            .mul_add(layout.banner_data_zoom, layout.pane_center_y),
        zoom: 0.8 * layout.banner_data_zoom,
    }
}

#[must_use]
pub fn double_scorebox_frame(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsFramePlacement {
    StepStatsFramePlacement {
        center_x: (notefield_width - 140.0).mul_add(layout.banner_data_zoom, layout.pane_center_x),
        center_y: 115.0f32.mul_add(-layout.banner_data_zoom, layout.pane_center_y),
        zoom: layout.banner_data_zoom,
    }
}

#[must_use]
pub fn double_sidepane_width(screen_w: f32, notefield_width: f32) -> f32 {
    ((screen_w - notefield_width) * 0.5).max(1.0)
}

#[must_use]
pub fn double_density_graph_rect(
    layout: DoubleStepStatsLayout,
    screen_w: f32,
    notefield_width: f32,
    current_graph_w: f32,
) -> StepStatsGraphRect {
    let sidepane_width = double_sidepane_width(screen_w, notefield_width);
    StepStatsGraphRect {
        x: layout.pane_center_x + 260.0,
        y: layout.pane_center_y + 40.0,
        w: density_graph_width(current_graph_w, sidepane_width, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_params() -> StepStatsPaneParams {
        StepStatsPaneParams {
            screen_w: 1280.0,
            screen_h: 720.0,
            screen_center_x: 640.0,
            screen_center_y: 360.0,
            playfield_center_x: 640.0,
            player_side: PlayerSide::P1,
            num_players: 1,
            notefield_width: Some(256.0),
            wide: true,
        }
    }

    #[test]
    fn centered_widescreen_pane_clamps_to_notefield_edge() {
        let layout = pane_layout(base_params());

        assert_eq!(layout.sidepane_width, 512.0);
        assert_eq!(layout.sidepane_center_x, 1024.0);
        assert_eq!(layout.sidepane_center_y, 440.0);
        assert!(layout.note_field_is_centered);
        assert!(!layout.is_ultrawide);
        assert_eq!(layout.banner_data_zoom, 0.925);
    }

    #[test]
    fn pane_anchors_use_available_width_without_scaling_widget_dimensions() {
        let mut params = base_params();
        params.screen_w = 640.0;
        params.screen_h = 480.0;
        params.screen_center_x = 320.0;
        params.screen_center_y = 240.0;
        params.playfield_center_x = 150.0;
        params.wide = false;
        let layout = pane_layout(params);

        assert_eq!(layout.sidepane_width, 320.0);
        assert_eq!(pane_anchor_x(layout, -200.0), 320.0);
        assert_eq!(pane_anchor_x(layout, 200.0), 640.0);
        assert_eq!(
            holds_mines_rolls_frame(layout, PlayerSide::P1),
            StepStatsFramePlacement {
                center_x: 604.0,
                center_y: 208.0,
                zoom: 1.0
            }
        );
        assert_eq!(
            pane_view_proj(640.0, 480.0),
            Metrics::centered(640.0, 480.0).projection()
        );
        assert_eq!(
            judgment_count_center_x(layout, PlayerSide::P1, false, 1),
            328.0
        );
    }

    #[test]
    fn ultrawide_versus_uses_outer_side_panes() {
        let layout = pane_layout(StepStatsPaneParams {
            screen_w: 2560.0,
            screen_h: 1080.0,
            screen_center_x: 1280.0,
            screen_center_y: 540.0,
            playfield_center_x: 1280.0,
            player_side: PlayerSide::P2,
            num_players: 2,
            notefield_width: Some(256.0),
            wide: true,
        });

        assert_eq!(layout.sidepane_width, 512.0);
        assert_eq!(layout.sidepane_center_x, 2304.0);
        assert_eq!(layout.sidepane_center_y, 620.0);
        assert!(layout.is_ultrawide);
        assert_eq!(layout.banner_data_zoom, 1.0);
    }

    #[test]
    fn song_info_zoom_tracks_banner_data_scale() {
        let layout = pane_layout(base_params());
        assert_eq!(song_info_text_zoom(layout), 0.8325);

        let mut params = base_params();
        params.playfield_center_x = 600.0;
        let off_center_layout = pane_layout(params);
        assert_eq!(off_center_layout.banner_data_zoom, 1.0);
        assert_eq!(song_info_text_zoom(off_center_layout), 0.75);
    }

    #[test]
    fn density_graph_width_uses_cached_or_pane_fallback() {
        assert_eq!(density_graph_width(320.0, 512.0, false), 320.0);
        assert_eq!(density_graph_width(0.0, 512.25, false), 512.0);
        assert_eq!(density_graph_width(0.0, 512.0, true), 486.4);
    }

    #[test]
    fn density_graph_rect_centers_on_pane() {
        let layout = StepStatsPaneLayout {
            sidepane_center_x: 100.0,
            sidepane_center_y: 200.0,
            sidepane_width: 80.0,
            note_field_is_centered: false,
            is_ultrawide: false,
            banner_data_zoom: 1.0,
        };

        assert_eq!(
            density_graph_rect(0.0, layout),
            StepStatsGraphRect {
                x: 60.0,
                y: 255.0,
                w: 80.0
            }
        );
    }

    #[test]
    fn banner_placement_mirrors_for_player_two() {
        let layout = StepStatsPaneLayout {
            sidepane_center_x: 500.0,
            sidepane_center_y: 300.0,
            sidepane_width: 400.0,
            note_field_is_centered: false,
            is_ultrawide: false,
            banner_data_zoom: 2.0,
        };

        assert_eq!(
            song_banner_placement(layout, true, PlayerSide::P1, 1),
            StepStatsSpritePlacement {
                x: 640.0,
                y: -100.0,
                zoom: 0.8
            }
        );
        assert_eq!(
            song_banner_placement(layout, true, PlayerSide::P2, 1),
            StepStatsSpritePlacement {
                x: 360.0,
                y: -100.0,
                zoom: 0.8
            }
        );
    }

    #[test]
    fn pack_banner_and_song_info_share_the_available_slide() {
        let layout = StepStatsPaneLayout {
            sidepane_center_x: 500.0,
            sidepane_center_y: 300.0,
            sidepane_width: 320.0,
            note_field_is_centered: false,
            is_ultrawide: false,
            banner_data_zoom: 1.0,
        };
        let slide = song_info_slide(layout, PlayerSide::P1, false, 1, true, 3, 355.0);
        let banner = pack_banner_placement(slide);
        let text = song_info_text_placement(slide, true, 0.75);

        assert_eq!(slide.left, 348.0);
        assert_eq!(slide.width, 289.0);
        assert!((slide.center_y + slide.height * 0.5 - 355.0).abs() < 0.001);
        assert_eq!(
            banner.x - STEP_STATS_BANNER_W * banner.zoom * 0.5,
            slide.left
        );
        assert_eq!(banner.y, slide.center_y);
        assert_eq!(banner.zoom, 0.25);
        assert!(text.labels_x >= banner.x + STEP_STATS_BANNER_W * banner.zoom * 0.5);
        assert!(text.values_x + text.values_max_width <= slide.left + slide.width);
    }

    #[test]
    fn song_info_without_pack_banner_uses_the_full_lane() {
        let slide = StepStatsSongInfoSlide {
            left: 448.0,
            center_y: 300.0,
            width: 192.0,
            height: 100.0,
            banner_width: 0.0,
        };
        let text = song_info_text_placement(slide, false, 0.74);

        assert_eq!(text.values_x, slide.left + STEP_STATS_SONG_INFO_PADDING);
        assert_eq!(text.values_max_width, slide.width - 2.0 * STEP_STATS_SONG_INFO_PADDING);
    }

    #[test]
    fn step_count_frames_follow_player_side_and_ultrawide() {
        let layout = StepStatsPaneLayout {
            sidepane_center_x: 500.0,
            sidepane_center_y: 300.0,
            sidepane_width: 400.0,
            note_field_is_centered: true,
            is_ultrawide: true,
            banner_data_zoom: 2.0,
        };

        assert_eq!(
            holds_mines_rolls_frame(layout, PlayerSide::P2),
            StepStatsFramePlacement {
                center_x: 330.0,
                center_y: 76.0,
                zoom: 2.0
            }
        );
        assert_eq!(
            scorebox_frame(layout, true, PlayerSide::P1, 2),
            StepStatsFramePlacement {
                center_x: 356.0,
                center_y: 70.0,
                zoom: 2.0
            }
        );
    }

    #[test]
    fn double_layout_uses_centered_pane_and_zoom_band() {
        let layout = double_pane_layout(640.0, 360.0, 1280.0, 720.0, 640.0);

        assert_eq!(
            layout,
            DoubleStepStatsLayout {
                pane_center_x: 640.0,
                pane_center_y: 440.0,
                note_field_is_centered: true,
                banner_data_zoom: 0.925
            }
        );
    }

    #[test]
    fn double_banner_and_pack_share_song_left_edge() {
        let layout = DoubleStepStatsLayout {
            pane_center_x: 640.0,
            pane_center_y: 440.0,
            note_field_is_centered: false,
            banner_data_zoom: 1.0,
        };

        assert_eq!(
            double_song_banner_placement(layout, 512.0),
            StepStatsSpritePlacement {
                x: 1012.0,
                y: 240.0,
                zoom: STEP_STATS_SONG_BANNER_ZOOM
            }
        );
        assert_eq!(
            double_pack_banner_placement(layout, 512.0),
            StepStatsSpritePlacement {
                x: 980.65,
                y: 460.0,
                zoom: 0.25
            }
        );
    }

    #[test]
    fn double_step_count_frames_match_theme_offsets() {
        let layout = DoubleStepStatsLayout {
            pane_center_x: 640.0,
            pane_center_y: 440.0,
            note_field_is_centered: false,
            banner_data_zoom: 1.0,
        };

        assert_eq!(
            double_holds_mines_rolls_frame(layout, 512.0),
            StepStatsFramePlacement {
                center_x: 340.0,
                center_y: 452.4,
                zoom: 0.8
            }
        );
        assert_eq!(
            double_scorebox_frame(layout, 512.0),
            StepStatsFramePlacement {
                center_x: 1012.0,
                center_y: 325.0,
                zoom: 1.0
            }
        );
    }

    #[test]
    fn double_density_graph_uses_sidepane_fallback_width() {
        let layout = DoubleStepStatsLayout {
            pane_center_x: 640.0,
            pane_center_y: 440.0,
            note_field_is_centered: false,
            banner_data_zoom: 1.0,
        };

        assert_eq!(double_sidepane_width(1280.0, 512.0), 384.0);
        assert_eq!(
            double_density_graph_rect(layout, 1280.0, 512.0, 0.0),
            StepStatsGraphRect {
                x: 900.0,
                y: 480.0,
                w: 364.8
            }
        );
    }

    #[test]
    fn centered_single_split_fits_vga_5_4_and_6_5() {
        for screen_w in [640.0, 600.0, 576.0] {
            let layout = centered_single_split_layout(
                screen_w * 0.5,
                240.0,
                screen_w,
                480.0,
                screen_w * 0.5,
                256.0,
                0.0,
            )
            .expect("centered split fits");

            assert!(layout.panel_width <= layout.sidepane_width);
            assert_eq!(
                layout.pane.banner_data_zoom,
                (layout.sidepane_width / 200.0).min(1.0)
            );
            assert!(layout.graph.x >= screen_w * 0.5 + 128.0);
            assert!(layout.graph.x + layout.graph.w <= screen_w);
            assert_eq!(layout.graph.y, 375.0);
            assert_eq!(layout.graph.y + 105.0, 480.0);

            let song_banner = layout.song_banner();
            let banner_half_width = STEP_STATS_BANNER_W * song_banner.zoom * 0.5;
            let right_panel_left = screen_w * 0.5 + 128.0;
            let banner_half_height =
                STEP_STATS_BANNER_H * STEP_STATS_SONG_BANNER_ZOOM * layout.pane.banner_data_zoom * 0.5;
            let expected_banner_top =
                240.0 + 80.0 - 200.0 - STEP_STATS_BANNER_H * STEP_STATS_SONG_BANNER_ZOOM * 0.5;
            assert!(song_banner.x - banner_half_width >= right_panel_left);
            assert!(song_banner.x + banner_half_width <= right_panel_left + layout.panel_width);
            assert!((song_banner.y - banner_half_height - expected_banner_top).abs() < 0.001);
            assert!(layout.holds_mines_rolls_frame().center_x < right_panel_left);
            assert!(layout.scorebox_frame().center_x > right_panel_left);
        }

        let six_five = centered_single_split_layout(
            288.0, 240.0, 576.0, 480.0, 288.0, 256.0, 0.0,
        )
        .expect("6:5 is the minimum supported ratio");
        assert!((six_five.pane.banner_data_zoom - 0.8).abs() < 0.001);
    }

    #[test]
    fn centered_single_split_hides_when_it_does_not_fit_or_is_wide() {
        for (screen_w, screen_h) in [(560.0, 480.0), (768.0, 480.0), (640.0, 400.0)] {
            assert!(
                centered_single_split_layout(
                    screen_w * 0.5,
                    screen_h * 0.5,
                    screen_w,
                    screen_h,
                    screen_w * 0.5,
                    256.0,
                    0.0,
                )
                .is_none()
            );
        }
    }
}
