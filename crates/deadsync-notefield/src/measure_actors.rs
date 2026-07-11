use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};

pub fn append_edit_measure_number(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    measure_index: Option<i64>,
    x: f32,
    y: f32,
    field_zoom: f32,
    z_measure_lines: i16,
    font: &'static str,
) {
    let Some(measure) = measure_index else {
        return;
    };
    if !edit_beat_bars || measure < 0 {
        return;
    }

    let mut text = TextBuilder::new();
    text.font(font);
    text.settext(measure.to_string().into());
    text.align(1.0, 0.5);
    text.horizalign(TextAlign::Right);
    text.xy(x, y);
    text.zoom((field_zoom * 0.9).clamp(0.35, 0.75));
    text.shadowlength(2.0);
    text.diffuse([1.0, 1.0, 1.0, 1.0]);
    text.z(z_measure_lines.saturating_add(1));
    actors.push(text.build(0));
}

pub fn append_beat_bar(
    actors: &mut Vec<Actor>,
    edit_beat_bars: bool,
    edit_bar_frame: u32,
    x_center: f32,
    y: f32,
    width: f32,
    field_zoom: f32,
    thickness: f32,
    alpha: f32,
    z_measure_lines: i16,
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
            z_measure_lines,
        );
    } else {
        append_measure_quad(
            actors,
            [0.5, 0.5],
            [x_center, y],
            [width, thickness],
            [1.0, 1.0, 1.0, alpha],
            z_measure_lines,
        );
    }
}

/// Colored measure cue line marking timing events such as BPM changes, stops,
/// delays, and scrolls.
pub fn append_cue_bar(
    actors: &mut Vec<Actor>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: [f32; 3],
    alpha: f32,
    z_measure_lines: i16,
) {
    append_measure_quad(
        actors,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [color[0], color[1], color[2], alpha],
        z_measure_lines,
    );
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
    z_measure_lines: i16,
) {
    match frame {
        0 | 1 => append_edit_bar_segment(
            actors,
            x_center,
            y,
            width,
            thickness,
            alpha,
            z_measure_lines,
        ),
        2 => append_dashed_edit_bar(
            actors,
            x_center,
            y,
            width,
            thickness,
            12.0 * field_zoom,
            8.0 * field_zoom,
            alpha,
            z_measure_lines,
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
            z_measure_lines,
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
    z_measure_lines: i16,
) {
    append_measure_quad(
        actors,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [1.0, 1.0, 1.0, alpha],
        z_measure_lines,
    );
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
    z_measure_lines: i16,
) {
    let dash = dash.max(1.0);
    let step = (dash + gap).max(dash + 1.0);
    let left = x_center - width * 0.5;
    let right = x_center + width * 0.5;
    let mut x = left;
    while x < right {
        let seg_w = dash.min(right - x);
        append_measure_quad(
            actors,
            [0.0, 0.5],
            [x, y],
            [seg_w, thickness],
            [1.0, 1.0, 1.0, alpha],
            z_measure_lines,
        );
        x += step;
    }
}

fn append_measure_quad(
    actors: &mut Vec<Actor>,
    align: [f32; 2],
    xy: [f32; 2],
    size: [f32; 2],
    diffuse: [f32; 4],
    z: i16,
) {
    let mut quad = SpriteBuilder::solid();
    quad.align(align[0], align[1]);
    quad.xy(xy[0], xy[1]);
    quad.size(size[0], size[1]);
    quad.diffuse(diffuse);
    quad.z(z);
    actors.push(quad.build(0));
}
