use crate::act;
use deadlib_present::actors::Actor;

use super::Z_MEASURE_LINES;

pub(super) fn append_edit_measure_number(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    measure_index: Option<i64>,
    x: f32,
    y: f32,
    field_zoom: f32,
) {
    let Some(measure) = measure_index else {
        return;
    };
    if !edit_beat_bars || measure < 0 {
        return;
    }
    actors.push(act!(text:
        font("miso"):
        settext(measure.to_string()):
        align(1.0, 0.5):
        horizalign(right):
        xy(x, y):
        zoom((field_zoom * 0.9).clamp(0.35, 0.75)):
        shadowlength(2.0):
        diffuse(1.0, 1.0, 1.0, 1.0):
        z(Z_MEASURE_LINES + 1)
    ));
}

pub(super) fn append_beat_bar(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    edit_bar_frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
) {
    if edit_beat_bars {
        append_edit_beat_bar(
            actors,
            edit_bar_frame,
            x_center,
            y,
            width,
            field_zoom,
            thickness,
            alpha,
        );
    } else {
        actors.push(act!(quad:
            align(0.5, 0.5): xy(x_center, y):
            zoomto(width, thickness):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(Z_MEASURE_LINES)
        ));
    }
}

/// Measure cues: colored lines marking timing events (BPM changes, stops,
/// delays, and scrolls). They use the measure-line layer, so when emitted after
/// the white line pass they sit on top of coinciding white lines.
pub(super) fn append_cue_bar(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: (f32, f32, f32),
    alpha: f32,
) {
    let (r, g, b) = color;
    actors.push(act!(quad:
        align(0.5, 0.5): xy(x_center, y):
        zoomto(width, thickness):
        diffuse(r, g, b, alpha):
        z(Z_MEASURE_LINES)
    ));
}

fn append_edit_beat_bar(
    actors: &mut Vec<Actor>,
    frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
) {
    match frame {
        0 | 1 => append_edit_bar_segment(actors, x_center, y, width, thickness, alpha),
        2 => append_dashed_edit_bar(
            actors,
            x_center,
            y,
            width,
            thickness,
            12.0 * field_zoom,
            8.0 * field_zoom,
            alpha,
        ),
        _ => append_dashed_edit_bar(
            actors,
            x_center,
            y,
            width,
            thickness,
            4.0 * field_zoom,
            6.0 * field_zoom,
            alpha,
        ),
    }
}

fn append_edit_bar_segment(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    alpha: f32,
) {
    actors.push(act!(quad:
        align(0.5, 0.5):
        xy(x_center, y):
        zoomto(width, thickness):
        diffuse(1.0, 1.0, 1.0, alpha):
        z(Z_MEASURE_LINES)
    ));
}

fn append_dashed_edit_bar(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    dash: f32,
    gap: f32,
    alpha: f32,
) {
    let dash = dash.max(1.0);
    let step = (dash + gap).max(dash + 1.0);
    let left = x_center - width * 0.5;
    let right = x_center + width * 0.5;
    let mut x = left;
    while x < right {
        let seg_w = dash.min(right - x);
        actors.push(act!(quad:
            align(0.0, 0.5):
            xy(x, y):
            zoomto(seg_w, thickness):
            diffuse(1.0, 1.0, 1.0, alpha):
            z(Z_MEASURE_LINES)
        ));
        x += step;
    }
}
