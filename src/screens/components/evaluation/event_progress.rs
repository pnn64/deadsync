use crate::act;
use crate::core::space::{screen_center_x, screen_center_y, screen_height};
use crate::game::{profile, scores};
use crate::ui::actors::{Actor, SizeSpec};

const ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const BODY_FONT_HEIGHT: f32 = 19.0;
const BODY_LINE_SPACING: f32 = 24.0;
const BODY_AVG_CHAR_WIDTH: f32 = 8.0;
const UPPER_ROW_HEIGHT: f32 = 25.0;
const OVERLAY_ROW_HEIGHT: f32 = 24.0;

#[inline(always)]
fn header_name(name: &str, is_doubles: bool) -> String {
    let mut text = name.replacen("ITL Online", "ITL", 1);
    if is_doubles && !text.contains("Doubles") {
        text.push_str(" Doubles");
    }
    text
}

#[inline(always)]
fn format_pct_hundredths(value: u32) -> String {
    format!("{}.{:02}%", value / 100, value % 100)
}

#[inline(always)]
fn format_signed_pct_hundredths(value: i32) -> String {
    let sign = if value < 0 { '-' } else { '+' };
    let abs = value.unsigned_abs();
    format!("({sign}{}.{:02}%)", abs / 100, abs % 100)
}

#[inline(always)]
fn format_signed_points(value: i32) -> String {
    format!("({value:+})")
}

#[inline(always)]
fn clear_type_name(clear_type: u8) -> &'static str {
    match clear_type {
        0 => "No Play",
        1 => "Clear",
        2 => "FC",
        3 => "FEC",
        4 => "FFC",
        5 => "FBFC",
        _ => "Clear",
    }
}

#[inline(always)]
fn build_box_body(progress: &scores::ItlEventProgress) -> String {
    format!(
        "EX Score: {} {}\n\
         Points: {} {}\n\n\
         Ranking Points: {} {}\n\
         Song Points: {} {}\n\
         EX Points: {} {}\n\
         Total Points: {} {}",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        progress.current_points,
        format_signed_points(progress.point_delta),
        progress.current_ranking_points,
        format_signed_points(progress.ranking_delta),
        progress.current_song_points,
        format_signed_points(progress.song_delta),
        progress.current_ex_points,
        format_signed_points(progress.ex_delta),
        progress.current_total_points,
        format_signed_points(progress.total_delta),
    )
}

#[inline(always)]
fn build_stat_improvements(progress: &scores::ItlEventProgress) -> Option<String> {
    let (Some(before), Some(after)) = (progress.clear_type_before, progress.clear_type_after)
    else {
        return None;
    };
    (after > before).then(|| {
        format!(
            "Clear Type: {} >>> {}",
            clear_type_name(before),
            clear_type_name(after)
        )
    })
}

#[inline(always)]
fn build_overlay_body(progress: &scores::ItlEventProgress) -> String {
    let mut text = format!(
        "EX Score: {} {}\n\
         Points: {} {}\n\n\
         Ranking Points: {} {}\n\
         Song Points: {} {}\n\
         EX Points: {} {}\n\
         Total Points: {} {}\n\n\
         You've passed the chart {} times",
        format_pct_hundredths(progress.score_hundredths),
        format_signed_pct_hundredths(progress.score_delta_hundredths),
        progress.current_points,
        format_signed_points(progress.point_delta),
        progress.current_ranking_points,
        format_signed_points(progress.ranking_delta),
        progress.current_song_points,
        format_signed_points(progress.song_delta),
        progress.current_ex_points,
        format_signed_points(progress.ex_delta),
        progress.current_total_points,
        format_signed_points(progress.total_delta),
        progress.total_passes,
    );
    if let Some(improvement) = build_stat_improvements(progress) {
        text.push_str("\n\n");
        text.push_str(improvement.as_str());
    }
    text
}

#[inline(always)]
fn quantize_zoom(zoom: f32) -> f32 {
    ((zoom * 20.0).floor() / 20.0).clamp(0.1, 1.0)
}

#[inline(always)]
fn fit_body_zoom(text: &str, pane_width: f32, pane_height: f32, row_height: f32) -> f32 {
    let lines: Vec<&str> = text.lines().collect();
    let line_count = lines.len().max(1) as f32;
    let max_line_chars = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(1) as f32;
    let block_height = BODY_FONT_HEIGHT + (line_count - 1.0).max(0.0) * BODY_LINE_SPACING;
    let fit_height = ((pane_height - 2.0 - row_height * 1.5) / block_height.max(1.0)).min(1.0);
    let fit_width = (pane_width / (max_line_chars.max(1.0) * BODY_AVG_CHAR_WIDTH)).min(1.0);
    quantize_zoom(fit_height.min(fit_width))
}

#[inline(always)]
fn build_header_text(text: String, pane_width: f32, y: f32, z: i16) -> Actor {
    act!(text:
        font("wendy"):
        settext(text):
        align(0.5, 0.5):
        xy(0.0, y):
        zoom(0.5):
        maxwidth((pane_width - 6.0) / 0.5):
        horizalign(center):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z)
    )
}

#[inline(always)]
fn build_body_text(
    text: String,
    pane_width: f32,
    pane_height: f32,
    row_height: f32,
    z: i16,
) -> Actor {
    let zoom = fit_body_zoom(text.as_str(), pane_width, pane_height, row_height);
    act!(text:
        font("miso"):
        settext(text):
        align(0.5, 0.0):
        xy(0.0, -pane_height * 0.5 + row_height * 1.5):
        zoom(zoom):
        wrapwidthpixels(pane_width / zoom):
        horizalign(left):
        valign(top):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(z)
    )
}

fn build_upper_panel(
    center_x: f32,
    center_y: f32,
    pane_width: f32,
    pane_height: f32,
    progress: &scores::ItlEventProgress,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let mut children = Vec::with_capacity(4);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width, pane_height):
        diffuse(1.0, 1.0, 1.0, 0.1):
        z(0)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width - border_width, pane_height - border_width):
        diffuse(0.0, 0.0, 0.0, 0.85):
        z(1)
    ));
    children.push(build_header_text(
        header_name(progress.name.as_str(), progress.is_doubles),
        pane_width,
        -pane_height * 0.5 + 15.0,
        2,
    ));
    children.push(build_body_text(
        build_box_body(progress),
        pane_width,
        pane_height,
        UPPER_ROW_HEIGHT,
        2,
    ));

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [center_x, center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z,
    }
}

fn build_overlay_panel(
    center_x: f32,
    center_y: f32,
    pane_width: f32,
    pane_height: f32,
    progress: &scores::ItlEventProgress,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let header_y = -pane_height * 0.5 + 12.0;
    let header_bar_y = -pane_height * 0.5 + OVERLAY_ROW_HEIGHT * 0.5;
    let mut children = Vec::with_capacity(8);
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width + border_width, pane_height + border_width + 1.0):
        diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
        z(0)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, 0.0):
        setsize(pane_width, pane_height):
        diffuse(BLACK[0], BLACK[1], BLACK[2], BLACK[3]):
        z(1)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width + border_width, OVERLAY_ROW_HEIGHT + border_width + 1.0):
        diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
        z(2)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width, OVERLAY_ROW_HEIGHT):
        diffuse(0.157, 0.157, 0.165, 1.0):
        z(3)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, header_bar_y):
        setsize(pane_width, OVERLAY_ROW_HEIGHT):
        diffuse(0.3, 0.3, 0.3, 0.55):
        fadebottom(1.0):
        z(3)
    ));
    children.push(build_header_text(
        header_name(progress.name.as_str(), progress.is_doubles),
        pane_width,
        header_y,
        4,
    ));
    children.push(act!(text:
        font("wendy"):
        settext("EX"):
        align(0.5, 0.5):
        xy(pane_width * 0.5 - 18.0, header_y):
        zoom(0.5):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(4)
    ));
    children.push(build_body_text(
        build_overlay_body(progress),
        pane_width,
        pane_height,
        OVERLAY_ROW_HEIGHT,
        4,
    ));

    Actor::Frame {
        align: [0.5, 0.5],
        offset: [center_x, center_y],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        children,
        background: None,
        z,
    }
}

pub fn build_itl_progress_box(
    side: profile::PlayerSide,
    single_player: bool,
    progress: &scores::ItlEventProgress,
) -> Vec<Actor> {
    let upper_origin_x = match side {
        profile::PlayerSide::P1 => screen_center_x() - 155.0,
        profile::PlayerSide::P2 => screen_center_x() + 155.0,
    };
    let dir = if side == profile::PlayerSide::P1 {
        -1.0
    } else {
        1.0
    };
    let (center_x, center_y, pane_width, pane_height) = if single_player {
        (upper_origin_x - 381.0 * dir, 109.0, 156.0, 144.0)
    } else {
        (upper_origin_x + 211.0 * dir, 274.0, 118.0, 180.0)
    };
    vec![build_upper_panel(
        center_x,
        center_y,
        pane_width,
        pane_height,
        progress,
        104,
    )]
}

pub fn build_itl_event_overlay(
    panels: &[(profile::PlayerSide, &scores::ItlEventProgress)],
) -> Vec<Actor> {
    if panels.is_empty() {
        return Vec::new();
    }

    let pane_width = if panels.len() == 1 { 330.0 } else { 230.0 };
    let pane_height = 360.0;
    let center_y = screen_center_y() - 15.0;
    let mut actors = Vec::with_capacity(2 + panels.len());
    actors.push(act!(quad:
        align(0.0, 0.0):
        xy(0.0, 0.0):
        zoomto(screen_center_x() * 2.0, screen_height()):
        diffuse(0.0, 0.0, 0.0, 0.8):
        z(2000)
    ));

    for (idx, (side, progress)) in panels.iter().enumerate() {
        let center_x = if panels.len() == 1 {
            screen_center_x()
        } else if idx == 0 && *side == profile::PlayerSide::P1 {
            screen_center_x() - 160.0
        } else if idx == 0 && *side == profile::PlayerSide::P2 {
            screen_center_x() + 160.0
        } else if *side == profile::PlayerSide::P1 {
            screen_center_x() - 160.0
        } else {
            screen_center_x() + 160.0
        };
        actors.push(build_overlay_panel(
            center_x,
            center_y,
            pane_width,
            pane_height,
            progress,
            2001,
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext("Press Start or Back to dismiss"):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(1.1):
        diffuse(WHITE[0], WHITE[1], WHITE[2], WHITE[3]):
        z(2002)
    ));

    actors
}
