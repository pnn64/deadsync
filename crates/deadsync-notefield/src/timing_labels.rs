use crate::{NotefieldComposeRequest, PreparedNotefield};
use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::dsl::TextBuilder;
use deadsync_rules::scroll::ScrollSpeedSetting;
use std::sync::Arc;

/// Theme-supplied appearance of a timing segment annotation.
#[derive(Clone, Copy, Debug)]
pub struct TimingLabelStyle {
    pub color: [f32; 4],
    pub left_side: bool,
    pub offset_x: f32,
}

/// Immutable actor-ready text compiled once when entering Practice.
#[derive(Clone, Debug)]
pub struct TimingSegmentLabel {
    pub text: Arc<str>,
    pub beat: f32,
    pub style: TimingLabelStyle,
}

pub(crate) fn compose_timing_labels<S>(
    actors: &mut Vec<Actor>,
    request: &NotefieldComposeRequest<'_, S>,
    prepared: &PreparedNotefield<'_, S>,
) {
    if request.timing_labels.is_empty() {
        return;
    }
    let Some(notes) = prepared.notes.as_ref() else {
        return;
    };
    let cols = &notes.col_offsets[..prepared.frame_plan.num_cols];
    let min_x = cols.iter().copied().fold(f32::INFINITY, f32::min);
    let max_x = cols.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let field_zoom = prepared.field_zoom;
    let transform = crate::lane_note_transform_cache(
        prepared.current_beat,
        crate::gameplay_visual_effect_params(&request.visual.visual, 0),
    );
    let width = (max_x - min_x + ScrollSpeedSetting::ARROW_SPACING * field_zoom)
        * crate::visual_arrow_effect_zoom_cached(0.0, transform);
    let glow_alpha = timing_label_glow_alpha(request.visual.elapsed_screen_s);

    // PARITY[ITGmania NoteField::draw_timing_segment_text]: annotations use
    // column zero's timing, reverse, Tipsy, and zoom, inside the field camera.
    // Practice remains in edit mode during playback, so both views draw them.
    for label in request.timing_labels {
        let raw_travel = notes.travel.raw_beat(label.beat);
        let travel = notes.travel.adjusted(raw_travel);
        if !prepared.draw_range.contains(travel) {
            continue;
        }
        let y = notes.travel.lane_y(
            0,
            prepared.field.column_receptor_ys[0],
            prepared.field.column_dirs[0],
            raw_travel,
        );
        let margin = ScrollSpeedSetting::ARROW_SPACING * field_zoom.abs();
        if !y.is_finite() || y < -margin || y > request.geometry.screen_height + margin {
            continue;
        }
        let zoom = field_zoom * crate::visual_arrow_effect_zoom_cached(travel, transform);
        let x = timing_label_x(prepared.field.playfield_center_x, width, zoom, label.style);
        let mut text = TextBuilder::new();
        text.font(request.hud_style.edit_measure_number_font);
        text.settext((&label.text).into());
        text.align(if label.style.left_side { 1.0 } else { 0.0 }, 0.5);
        text.horizalign(if label.style.left_side {
            TextAlign::Right
        } else {
            TextAlign::Left
        });
        text.xy(x, y);
        text.zoom(zoom);
        text.wrapwidthpixels(300.0);
        text.diffuse(label.style.color);
        text.glow([1.0, 1.0, 1.0, glow_alpha]);
        text.shadowlength(2.0);
        text.z(crate::style::MEASURE_LINE_Z + 2);
        actors.push(text.build(0));
    }
}

fn timing_label_x(center_x: f32, width: f32, zoom: f32, style: TimingLabelStyle) -> f32 {
    let side = if style.left_side { -1.0 } else { 1.0 };
    center_x + side * style.offset_x.mul_add(zoom, width * 0.5)
}

fn timing_label_glow_alpha(elapsed: f32) -> f32 {
    let phase = elapsed * std::f32::consts::TAU / 6.0;
    phase.cos().mul_add(0.5, 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_label_x_uses_inherited_side_offsets() {
        let style = TimingLabelStyle {
            color: [1.0; 4],
            left_side: true,
            offset_x: 60.0,
        };
        assert_eq!(timing_label_x(400.0, 160.0, 0.5, style), 290.0);
        assert_eq!(
            timing_label_x(
                400.0,
                160.0,
                0.5,
                TimingLabelStyle {
                    left_side: false,
                    offset_x: 30.0,
                    ..style
                }
            ),
            495.0
        );
    }

    #[test]
    fn timing_label_glow_uses_six_second_cycle() {
        assert_eq!(timing_label_glow_alpha(0.0), 1.0);
        assert!((timing_label_glow_alpha(3.0) - 0.0).abs() < 0.000_001);
        assert!((timing_label_glow_alpha(6.0) - 1.0).abs() < 0.000_001);
    }
}
