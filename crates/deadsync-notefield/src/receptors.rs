use crate::style::*;
use crate::*;

pub fn receptor_row_center(
    field_center_x: f32,
    local_col: usize,
    receptor_y: f32,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    move_y: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
    tipsy: f32,
) -> [f32; 2] {
    let x = field_center_x
        + note_x_offset(
            local_col,
            0.0,
            beat_factor_value,
            elapsed,
            col_offsets,
            invert,
            tornado,
            move_x,
            params,
            tiny_zoom,
        );
    let y = receptor_y
        + move_col_extra(move_y, local_col)
        + tipsy_y_extra(local_col, beat_factor_value, tipsy);
    [x, y]
}

pub fn hold_indicator_column_x(
    field_center_x: f32,
    local_col: usize,
    beat_factor_value: f32,
    elapsed: f32,
    col_offsets: &[f32],
    invert: &[f32],
    tornado: &[TornadoBounds],
    move_x: &[f32],
    params: NoteXParams,
    tiny_zoom: f32,
) -> f32 {
    field_center_x
        + note_x_offset(
            local_col,
            0.0,
            beat_factor_value,
            elapsed,
            col_offsets,
            invert,
            tornado,
            move_x,
            params,
            tiny_zoom,
        )
}

#[allow(dead_code)]
fn _uses_style() -> f32 {
    CENTER_LINE_Y
}
