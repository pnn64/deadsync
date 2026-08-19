use deadlib_present::actors::{Actor, FlatDraw, FlatSprite, SpriteSource, TextAlign, TextContent};
use deadlib_present::dsl::TextBuilder;
use deadlib_render_core::BlendMode;

pub(crate) fn append_edit_measure_number(
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
    text.settext(edit_measure_text(measure as u64));
    text.align(1.0, 0.5);
    text.horizalign(TextAlign::Right);
    text.xy(x, y);
    text.zoom((field_zoom * 0.9).clamp(0.35, 0.75));
    text.shadowlength(2.0);
    text.diffuse([1.0, 1.0, 1.0, 1.0]);
    text.z(z_measure_lines.saturating_add(1));
    actors.push(text.build(0));
}

fn edit_measure_text(measure: u64) -> TextContent {
    u32::try_from(measure)
        .map(TextContent::inline_u32)
        .unwrap_or_else(|_| measure.to_string().into())
}

pub(crate) fn append_beat_bar(
    draws: &mut Vec<FlatDraw>,
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
            draws,
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
            draws,
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
pub(crate) fn append_cue_bar(
    draws: &mut Vec<FlatDraw>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    color: [f32; 3],
    alpha: f32,
    z_measure_lines: i16,
) {
    append_measure_quad(
        draws,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [color[0], color[1], color[2], alpha],
        z_measure_lines,
    );
}

fn append_edit_beat_bar(
    draws: &mut Vec<FlatDraw>,
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
        0 | 1 => {
            append_edit_bar_segment(draws, x_center, y, width, thickness, alpha, z_measure_lines)
        }
        2 => append_dashed_edit_bar(
            draws,
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
            draws,
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
    draws: &mut Vec<FlatDraw>,
    x_center: f32,
    y: f32,
    width: f32,
    thickness: f32,
    alpha: f32,
    z_measure_lines: i16,
) {
    append_measure_quad(
        draws,
        [0.5, 0.5],
        [x_center, y],
        [width, thickness],
        [1.0, 1.0, 1.0, alpha],
        z_measure_lines,
    );
}

fn append_dashed_edit_bar(
    draws: &mut Vec<FlatDraw>,
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
            draws,
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
    draws: &mut Vec<FlatDraw>,
    align: [f32; 2],
    xy: [f32; 2],
    size: [f32; 2],
    diffuse: [f32; 4],
    z: i16,
) {
    draws.push(FlatDraw::Sprite(FlatSprite {
        center: [
            xy[0] + (0.5 - align[0]) * size[0],
            xy[1] + (0.5 - align[1]) * size[1],
        ],
        world_z: 0.0,
        size,
        source: SpriteSource::Solid,
        tint: diffuse,
        glow: [1.0, 1.0, 1.0, 0.0],
        uv_rect: [0.0, 0.0, 1.0, 1.0],
        flip_x: false,
        flip_y: false,
        fade: [0.0; 4],
        blend: BlendMode::Alpha,
        rot_y_deg: 0.0,
        rot_z_deg: 0.0,
        z,
    }));
}
