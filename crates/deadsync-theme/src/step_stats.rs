use deadsync_profile::PlayerSide;

pub const STEP_STATS_SONG_BANNER_ZOOM: f32 = 0.4;
pub const STEP_STATS_BANNER_W: f32 = 418.0;
pub const STEP_STATS_BANNER_H: f32 = 164.0;

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
pub struct StepStatsGraphRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsSpritePlacement {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepStatsFramePlacement {
    pub center_x: f32,
    pub center_y: f32,
    pub zoom: f32,
}

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
            PlayerSide::P1 => params.screen_center_x + nf_width + (sidepane_width - nf_width) * 0.5,
            PlayerSide::P2 => params.screen_center_x - nf_width - (sidepane_width - nf_width) * 0.5,
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
        0.825 + (0.925 - 0.825) * t
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

pub fn song_info_text_zoom(layout: StepStatsPaneLayout, screen_aspect_ratio: f32) -> f32 {
    let mut zoom = 0.75;
    if layout.note_field_is_centered {
        zoom = if screen_aspect_ratio > 1.7 { 0.9 } else { 0.95 };
    }
    zoom * layout.banner_data_zoom
}

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

pub fn density_graph_rect(current_graph_w: f32, layout: StepStatsPaneLayout) -> StepStatsGraphRect {
    let graph_w = density_graph_width(current_graph_w, layout.sidepane_width, false);
    StepStatsGraphRect {
        x: layout.sidepane_center_x - graph_w * 0.5,
        y: layout.sidepane_center_y + 55.0,
        w: graph_w,
    }
}

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
        x: layout.sidepane_center_x + local_x * layout.banner_data_zoom,
        y: layout.sidepane_center_y - 200.0 * layout.banner_data_zoom,
        zoom: STEP_STATS_SONG_BANNER_ZOOM * layout.banner_data_zoom,
    }
}

pub fn pack_banner_placement(
    layout: StepStatsPaneLayout,
    player_side: PlayerSide,
) -> StepStatsSpritePlacement {
    let final_size = if layout.note_field_is_centered {
        0.2
    } else {
        0.25
    };
    let final_offset = if layout.note_field_is_centered {
        -115.0
    } else {
        -160.0
    };
    let side_sign = match player_side {
        PlayerSide::P1 => 1.0,
        PlayerSide::P2 => -1.0,
    };
    StepStatsSpritePlacement {
        x: layout.sidepane_center_x + final_offset * side_sign * layout.banner_data_zoom,
        y: layout.sidepane_center_y + 20.0 * layout.banner_data_zoom,
        zoom: final_size * layout.banner_data_zoom,
    }
}

pub fn holds_mines_rolls_frame(
    layout: StepStatsPaneLayout,
    player_side: PlayerSide,
) -> StepStatsFramePlacement {
    let local_x = match player_side {
        PlayerSide::P1 => 155.0,
        PlayerSide::P2 => -85.0,
    };
    StepStatsFramePlacement {
        center_x: layout.sidepane_center_x + local_x * layout.banner_data_zoom,
        center_y: layout.sidepane_center_y - 112.0 * layout.banner_data_zoom,
        zoom: layout.banner_data_zoom,
    }
}

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
        local_x += 2.0 * x_sign;
    }
    if layout.is_ultrawide && num_players > 1 {
        local_x = -local_x;
    }
    StepStatsFramePlacement {
        center_x: layout.sidepane_center_x + local_x * layout.banner_data_zoom,
        center_y: layout.sidepane_center_y - 115.0 * layout.banner_data_zoom,
        zoom: layout.banner_data_zoom,
    }
}

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
        0.825 + (0.925 - 0.825) * t
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

pub fn double_song_banner_placement(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsSpritePlacement {
    StepStatsSpritePlacement {
        x: layout.pane_center_x + (notefield_width - 140.0) * layout.banner_data_zoom,
        y: layout.pane_center_y - 200.0 * layout.banner_data_zoom,
        zoom: STEP_STATS_SONG_BANNER_ZOOM * layout.banner_data_zoom,
    }
}

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
        x: song_banner.x - song_w * 0.5 + pack_w * 0.5,
        y: layout.pane_center_y + 20.0 * layout.banner_data_zoom,
        zoom: final_size * layout.banner_data_zoom,
    }
}

pub fn double_holds_mines_rolls_frame(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsFramePlacement {
    StepStatsFramePlacement {
        center_x: layout.pane_center_x + (-notefield_width + 212.0) * layout.banner_data_zoom,
        center_y: layout.pane_center_y + (-10.0 + 0.8 * 28.0) * layout.banner_data_zoom,
        zoom: 0.8 * layout.banner_data_zoom,
    }
}

pub fn double_scorebox_frame(
    layout: DoubleStepStatsLayout,
    notefield_width: f32,
) -> StepStatsFramePlacement {
    StepStatsFramePlacement {
        center_x: layout.pane_center_x + (notefield_width - 140.0) * layout.banner_data_zoom,
        center_y: layout.pane_center_y - 115.0 * layout.banner_data_zoom,
        zoom: layout.banner_data_zoom,
    }
}

pub fn double_sidepane_width(screen_w: f32, notefield_width: f32) -> f32 {
    ((screen_w - notefield_width) * 0.5).max(1.0)
}

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
    fn song_info_zoom_uses_centered_aspect_ratio_band() {
        let mut layout = pane_layout(base_params());
        assert_eq!(song_info_text_zoom(layout, 16.0 / 9.0), 0.9 * 0.925);

        layout.note_field_is_centered = false;
        assert_eq!(song_info_text_zoom(layout, 16.0 / 9.0), 0.75 * 0.925);
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
    fn pack_banner_placement_uses_centered_scale_and_offset() {
        let layout = StepStatsPaneLayout {
            sidepane_center_x: 500.0,
            sidepane_center_y: 300.0,
            sidepane_width: 400.0,
            note_field_is_centered: true,
            is_ultrawide: false,
            banner_data_zoom: 2.0,
        };

        assert_eq!(
            pack_banner_placement(layout, PlayerSide::P1),
            StepStatsSpritePlacement {
                x: 270.0,
                y: 340.0,
                zoom: 0.4
            }
        );
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
}
