use crate::act;
use crate::core::space::{screen_center_x, screen_center_y, screen_height};
use crate::game::{profile, scores};
use crate::ui::actors::{Actor, SizeSpec};

const ITL_PINK: [f32; 4] = [1.0, 0.2, 0.406, 1.0];
const POSITIVE_GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const NEGATIVE_RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

struct RowData {
    label: &'static str,
    current: String,
    delta: String,
    delta_value: i32,
}

#[inline(always)]
fn short_event_name(name: &str, is_doubles: bool) -> String {
    let mut text = name.replacen("ITL Online", "ITL", 1);
    if is_doubles {
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
fn delta_color(delta: i32) -> [f32; 4] {
    if delta > 0 {
        POSITIVE_GREEN
    } else if delta < 0 {
        NEGATIVE_RED
    } else {
        WHITE
    }
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
fn build_rows(progress: &scores::ItlEventProgress, compact: bool) -> [RowData; 6] {
    let rank_label = if compact {
        "Rank Pts"
    } else {
        "Ranking Points"
    };
    let song_label = if compact { "Song Pts" } else { "Song Points" };
    let ex_label = if compact { "EX Pts" } else { "EX Points" };
    let total_label = if compact { "Total Pts" } else { "Total Points" };
    [
        RowData {
            label: "EX Score",
            current: format_pct_hundredths(progress.score_hundredths),
            delta: format_signed_pct_hundredths(progress.score_delta_hundredths),
            delta_value: progress.score_delta_hundredths,
        },
        RowData {
            label: "Points",
            current: progress.current_points.to_string(),
            delta: format_signed_points(progress.point_delta),
            delta_value: progress.point_delta,
        },
        RowData {
            label: rank_label,
            current: progress.current_ranking_points.to_string(),
            delta: format_signed_points(progress.ranking_delta),
            delta_value: progress.ranking_delta,
        },
        RowData {
            label: song_label,
            current: progress.current_song_points.to_string(),
            delta: format_signed_points(progress.song_delta),
            delta_value: progress.song_delta,
        },
        RowData {
            label: ex_label,
            current: progress.current_ex_points.to_string(),
            delta: format_signed_points(progress.ex_delta),
            delta_value: progress.ex_delta,
        },
        RowData {
            label: total_label,
            current: progress.current_total_points.to_string(),
            delta: format_signed_points(progress.total_delta),
            delta_value: progress.total_delta,
        },
    ]
}

fn build_panel(
    center_x: f32,
    center_y: f32,
    pane_width: f32,
    pane_height: f32,
    progress: &scores::ItlEventProgress,
    compact: bool,
    show_passes: bool,
    z: i16,
) -> Actor {
    let border_width = 2.0;
    let header_y = -pane_height * 0.5 + 14.0;
    let body_start_y = if compact {
        -pane_height * 0.5 + 40.0
    } else {
        -pane_height * 0.5 + 60.0
    };
    let row_step = if compact { 19.0 } else { 31.0 };
    let label_zoom = if compact { 0.41 } else { 0.60 };
    let value_zoom = if compact { 0.39 } else { 0.56 };
    let delta_zoom = if compact { 0.33 } else { 0.46 };
    let label_x = -pane_width * 0.5 + 8.0;
    let value_x = if compact {
        pane_width * 0.5 - 38.0
    } else {
        pane_width * 0.5 - 60.0
    };
    let delta_x = pane_width * 0.5 - 8.0;
    let rows = build_rows(progress, compact);

    let mut children = Vec::with_capacity(24);
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
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, -pane_height * 0.5 + 12.5):
        setsize(pane_width - border_width, 25.0):
        diffuse(0.157, 0.157, 0.165, 1.0):
        z(2)
    ));
    children.push(act!(quad:
        align(0.5, 0.5):
        xy(0.0, -pane_height * 0.5 + 12.5):
        setsize(pane_width - border_width, 25.0):
        diffuse(0.3, 0.3, 0.3, 0.55):
        fadebottom(1.0):
        z(2)
    ));
    children.push(act!(sprite("ITL.png"):
        align(0.5, 0.5):
        xy(0.0, 0.0):
        zoom(if compact { 0.16 } else { 0.24 }):
        diffuse(1.0, 1.0, 1.0, if compact { 0.14 } else { 0.18 }):
        z(2)
    ));
    children.push(act!(text:
        font("wendy"):
        settext(short_event_name(progress.name.as_str(), progress.is_doubles)):
        align(0.5, 0.5):
        xy(0.0, header_y):
        zoom(if compact { 0.34 } else { 0.52 }):
        maxwidth((pane_width - 8.0) / if compact { 0.34 } else { 0.52 }):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(3)
    ));

    for (idx, row) in rows.iter().enumerate() {
        let y = body_start_y + row_step * idx as f32;
        children.push(act!(text:
            font("miso"):
            settext(row.label):
            align(0.0, 0.5):
            xy(label_x, y):
            zoom(label_zoom):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(3)
        ));
        children.push(act!(text:
            font("miso"):
            settext(row.current.clone()):
            align(1.0, 0.5):
            xy(value_x, y):
            zoom(value_zoom):
            diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
            horizalign(right):
            z(3)
        ));
        children.push(act!(text:
            font("miso"):
            settext(row.delta.clone()):
            align(1.0, 0.5):
            xy(delta_x, y):
            zoom(delta_zoom):
            diffuse(
                delta_color(row.delta_value)[0],
                delta_color(row.delta_value)[1],
                delta_color(row.delta_value)[2],
                delta_color(row.delta_value)[3]
            ):
            horizalign(right):
            z(3)
        ));
    }

    if show_passes {
        let passes_y = body_start_y + row_step * rows.len() as f32 + 12.0;
        children.push(act!(text:
            font("miso"):
            settext("Chart Passes"):
            align(0.0, 0.5):
            xy(label_x, passes_y):
            zoom(if compact { 0.41 } else { 0.52 }):
            diffuse(1.0, 1.0, 1.0, 1.0):
            z(3)
        ));
        children.push(act!(text:
            font("miso"):
            settext(progress.total_passes.to_string()):
            align(1.0, 0.5):
            xy(delta_x, passes_y):
            zoom(if compact { 0.39 } else { 0.50 }):
            diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
            horizalign(right):
            z(3)
        ));

        if let (Some(before), Some(after)) = (progress.clear_type_before, progress.clear_type_after)
            && after > before
        {
            let clear_y = passes_y + 28.0;
            children.push(act!(text:
                font("miso"):
                settext("Clear Type"):
                align(0.0, 0.5):
                xy(label_x, clear_y):
                zoom(if compact { 0.41 } else { 0.52 }):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(3)
            ));
            children.push(act!(text:
                font("miso"):
                settext(format!("{} -> {}", clear_type_name(before), clear_type_name(after))):
                align(1.0, 0.5):
                xy(delta_x, clear_y):
                zoom(if compact { 0.33 } else { 0.44 }):
                diffuse(ITL_PINK[0], ITL_PINK[1], ITL_PINK[2], ITL_PINK[3]):
                horizalign(right):
                z(3)
            ));
        }
    }

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
    vec![build_panel(
        center_x,
        center_y,
        pane_width,
        pane_height,
        progress,
        true,
        false,
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
        actors.push(build_panel(
            center_x,
            center_y,
            pane_width,
            pane_height,
            progress,
            false,
            true,
            2001,
        ));
    }

    actors.push(act!(text:
        font("miso"):
        settext("Press Start or Back to dismiss"):
        align(0.5, 0.5):
        xy(screen_center_x(), screen_height() - 50.0):
        zoom(0.75):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(2002)
    ));

    actors
}
