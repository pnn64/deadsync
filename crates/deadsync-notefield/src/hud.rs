use crate::{
    LayoutMiniIndicatorPosition, ZmodMeasureCounterText, stream_segment_index_exclusive_end,
    zmod_broken_run_counter_text, zmod_broken_run_segment, zmod_measure_counter_text,
    zmod_run_timer_index,
};
use deadlib_present::actors::{Actor, TextAlign};
use deadlib_present::dsl::TextBuilder;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::StreamSegment;
use deadsync_theme::{CounterHudStyle, MiniIndicatorStyle};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct CounterHudRequest<'a> {
    pub style: CounterHudStyle,
    pub segments: &'a [StreamSegment],
    pub current_beat: f32,
    pub current_display_beat: f32,
    pub current_bpm: f32,
    pub music_rate: f32,
    pub lookahead: u8,
    pub multiplier: f32,
    pub vertical: bool,
    pub left: bool,
    pub broken_run: bool,
    pub run_timer: bool,
    pub measure_counter_y: Option<f32>,
    pub subtractive_scoring_y: f32,
    pub playfield_center_x: f32,
    pub field_zoom: f32,
    pub font: &'static str,
    pub counter_text: fn(ZmodMeasureCounterText) -> Arc<str>,
    pub timer_text: fn(i32, i32, bool) -> Arc<str>,
}

/// Compose the canonical measure counter, broken-run counter, and run timer.
/// The caller supplies resolved gameplay values, theme metrics, and cached text
/// formatters; placement and actor construction stay inside the notefield.
pub fn compose_counter_hud(actors: &mut Vec<Actor>, request: CounterHudRequest<'_>) {
    let segments = request.segments;
    if segments.is_empty() {
        return;
    }

    let beat_floor = request.current_beat.floor();
    let current_measure = beat_floor / 4.0;
    let base_index = stream_segment_index_exclusive_end(segments, current_measure);
    let mut column_width = ScrollSpeedSetting::ARROW_SPACING * request.field_zoom;
    if request.left {
        column_width *= request.style.left_column_scale;
    }

    if let Some(counter_y) = request.measure_counter_y {
        append_measure_counters(
            actors,
            request,
            beat_floor,
            current_measure,
            base_index,
            column_width,
            counter_y,
        );
        append_broken_counter(actors, request, current_measure, column_width, counter_y);
    }
    append_run_timer(actors, request, current_measure, column_width);
}

#[allow(clippy::too_many_arguments)]
fn append_measure_counters(
    actors: &mut Vec<Actor>,
    request: CounterHudRequest<'_>,
    beat_floor: f32,
    current_measure: f32,
    base_index: usize,
    column_width: f32,
    counter_y: f32,
) {
    for j in (0..=request.lookahead).rev() {
        let segment_index = base_index + j as usize;
        let Some(segment) = request.segments.get(segment_index).copied() else {
            continue;
        };
        let is_lookahead = j != 0;
        let Some(text_kind) = zmod_measure_counter_text(
            beat_floor,
            current_measure,
            request.segments,
            segment_index,
            is_lookahead,
            request.lookahead.into(),
            request.multiplier,
        ) else {
            continue;
        };
        let is_ratio = matches!(text_kind, ZmodMeasureCounterText::Ratio { .. });
        let color = if segment.is_break {
            if is_lookahead {
                request.style.break_lookahead_color
            } else {
                request.style.break_current_color
            }
        } else if is_lookahead {
            request.style.stream_lookahead_color
        } else if is_ratio {
            request.style.ratio_color
        } else {
            request.style.total_color
        };
        let zoom = request.style.base_zoom - request.style.lookahead_zoom_step * f32::from(j);
        let mut x = request.playfield_center_x;
        let mut y = counter_y;
        if request.vertical {
            y += request.style.vertical_step_y * f32::from(j);
        } else {
            let denominator = if request.lookahead == 0 {
                1.0
            } else {
                f32::from(request.lookahead)
            };
            x += (column_width / denominator) * request.style.horizontal_span * f32::from(j);
        }
        if request.left {
            x -= column_width;
        }
        append_hud_text(
            actors,
            request.style,
            request.font,
            (request.counter_text)(text_kind),
            [x, y],
            [0.5, 0.5],
            zoom,
            color,
        );
    }
}

fn append_broken_counter(
    actors: &mut Vec<Actor>,
    request: CounterHudRequest<'_>,
    current_measure: f32,
    column_width: f32,
    counter_y: f32,
) {
    if !request.broken_run {
        return;
    }
    let Some((segment_index, broken_end, is_broken)) =
        zmod_broken_run_segment(request.segments, current_measure)
    else {
        return;
    };
    if request.segments[segment_index].is_break || !is_broken {
        return;
    }
    let Some(text_kind @ ZmodMeasureCounterText::Ratio { .. }) =
        zmod_broken_run_counter_text(current_measure, request.segments, segment_index, broken_end)
    else {
        return;
    };

    let mut x = request.playfield_center_x;
    let mut y = counter_y + request.style.broken_y_offset;
    if request.vertical {
        y += request.style.broken_vertical_y_offset;
        x += column_width * request.style.broken_vertical_x_scale;
    }
    if request.left {
        x -= column_width;
    }
    append_hud_text(
        actors,
        request.style,
        request.font,
        (request.counter_text)(text_kind),
        [x, y],
        [0.5, 0.5],
        request.style.base_zoom,
        request.style.broken_color,
    );
}

fn append_run_timer(
    actors: &mut Vec<Actor>,
    request: CounterHudRequest<'_>,
    current_measure: f32,
    column_width: f32,
) {
    if !request.run_timer {
        return;
    }
    let Some(segment_index) = zmod_run_timer_index(request.segments, current_measure) else {
        return;
    };
    let segment = request.segments[segment_index];
    if segment.is_break {
        return;
    }
    let current_bps = request.current_bpm / 60.0;
    if !current_bps.is_finite()
        || current_bps <= 0.0
        || !request.music_rate.is_finite()
        || request.music_rate <= 0.0
    {
        return;
    }

    let measure_seconds = 4.0 / (current_bps * request.music_rate);
    let current_time = request.current_display_beat / (current_bps * request.music_rate);
    let segment_len = (((segment.end - segment.start) as f32) * measure_seconds).ceil() as i32;
    let total = (request.timer_text)(segment_len, 60, false);
    let remaining = (((segment.end as f32) * measure_seconds) - current_time)
        .ceil()
        .max(0.0) as i32;
    let text = if remaining > segment_len {
        total
    } else if remaining < 1 {
        (request.timer_text)(0, 59, true)
    } else {
        (request.timer_text)(remaining, 59, true)
    };
    let color = if text.contains(' ') {
        request.style.run_active_color
    } else {
        request.style.run_inactive_color
    };
    let mut x = request.playfield_center_x;
    if request.left {
        x -= column_width;
    }
    append_hud_text(
        actors,
        request.style,
        request.font,
        text,
        [x, request.subtractive_scoring_y],
        [0.5, 0.5],
        request.style.base_zoom,
        color,
    );
}

fn append_hud_text(
    actors: &mut Vec<Actor>,
    style: CounterHudStyle,
    font: &'static str,
    content: Arc<str>,
    offset: [f32; 2],
    align: [f32; 2],
    zoom: f32,
    color: [f32; 4],
) {
    let mut text = TextBuilder::new();
    text.font(font);
    text.settext(content.into());
    text.align(align[0], align[1]);
    text.horizalign(TextAlign::Center);
    text.xy(offset[0], offset[1]);
    text.zoom(zoom);
    text.shadowlength(style.shadow_len);
    text.diffuse(color);
    text.z(style.text_z);
    actors.push(text.build(0));
}

pub struct MiniIndicatorRequest {
    pub style: MiniIndicatorStyle,
    pub text: Arc<str>,
    pub color: [f32; 4],
    pub failed: bool,
    pub position: LayoutMiniIndicatorPosition,
    pub counter_left: bool,
    pub playfield_center_x: f32,
    pub field_zoom: f32,
    pub layout_add_x: f32,
    pub y: f32,
    pub zoom: f32,
    pub font: &'static str,
}

/// Compose the canonical gameplay mini score indicator.
pub fn compose_mini_indicator(actors: &mut Vec<Actor>, request: MiniIndicatorRequest) {
    let color = if request.failed {
        [
            request.style.failed_color[0],
            request.style.failed_color[1],
            request.style.failed_color[2],
            request.color[3],
        ]
    } else {
        request.color
    };
    let column_width = ScrollSpeedSetting::ARROW_SPACING * request.field_zoom;
    let mut x = request.playfield_center_x + column_width * request.style.column_offset;
    if request.position == LayoutMiniIndicatorPosition::UnderUpArrow {
        x += request.style.under_up_x_offset + request.layout_add_x;
    }
    let align_x = if request.counter_left {
        0.5
    } else {
        x += request.style.unanchored_x_offset;
        0.0
    };

    let mut text = TextBuilder::new();
    text.font(request.font);
    text.settext(request.text.into());
    text.align(align_x, 0.5);
    text.xy(x, request.y);
    text.zoom(request.zoom);
    text.shadowlength(request.style.shadow_len);
    text.diffuse(color);
    text.z(request.style.text_z);
    actors.push(text.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counter_style() -> CounterHudStyle {
        CounterHudStyle {
            text_z: 85,
            shadow_len: 1.0,
            base_zoom: 0.35,
            lookahead_zoom_step: 0.05,
            vertical_step_y: 20.0,
            left_column_scale: 4.0 / 3.0,
            horizontal_span: 2.0,
            break_lookahead_color: [0.4, 0.4, 0.4, 1.0],
            break_current_color: [0.5, 0.5, 0.5, 1.0],
            stream_lookahead_color: [0.45, 0.45, 0.45, 1.0],
            ratio_color: [1.0, 1.0, 1.0, 1.0],
            total_color: [0.5, 0.5, 0.5, 1.0],
            broken_y_offset: 15.0,
            broken_vertical_y_offset: -15.0,
            broken_vertical_x_scale: 4.0 / 3.0,
            broken_color: [1.0, 1.0, 1.0, 0.7],
            run_active_color: [1.0, 1.0, 1.0, 1.0],
            run_inactive_color: [0.5, 0.5, 0.5, 1.0],
        }
    }

    fn counter_text(value: ZmodMeasureCounterText) -> Arc<str> {
        match value {
            ZmodMeasureCounterText::Ratio { current, total } => {
                Arc::from(format!("{current}/{total}"))
            }
            ZmodMeasureCounterText::Break(value) => Arc::from(format!("({value})")),
            ZmodMeasureCounterText::Total(value) => Arc::from(value.to_string()),
        }
    }

    fn timer_text(value: i32, _mode: i32, active: bool) -> Arc<str> {
        Arc::from(if active {
            format!(" {value}")
        } else {
            value.to_string()
        })
    }

    fn assert_text(
        actor: &Actor,
        content: &str,
        offset: [f32; 2],
        color: [f32; 4],
        zoom: f32,
        align_x: f32,
        text_align: TextAlign,
    ) {
        match actor {
            Actor::Text {
                align,
                offset: actual_offset,
                color: actual_color,
                font,
                content: actual_content,
                align_text,
                z,
                scale,
                shadow_len,
                ..
            } => {
                assert_eq!(*align, [align_x, 0.5]);
                assert_eq!(*actual_offset, offset);
                assert_eq!(*actual_color, color);
                assert_eq!(*font, "hud-font");
                assert_eq!(actual_content.as_str(), content);
                assert_eq!(*align_text, text_align);
                assert_eq!(*z, 85);
                assert!((scale[0] - zoom).abs() <= 1e-6);
                assert!((scale[1] - zoom).abs() <= 1e-6);
                assert_eq!(*shadow_len, [1.0, -1.0]);
            }
            other => panic!("expected HUD text, got {other:?}"),
        }
    }

    #[test]
    fn measure_counter_actor_fingerprint_preserves_order_and_lookahead() {
        let segments = [
            StreamSegment {
                start: 0,
                end: 8,
                is_break: false,
            },
            StreamSegment {
                start: 8,
                end: 12,
                is_break: true,
            },
        ];
        let mut actors = Vec::new();
        compose_counter_hud(
            &mut actors,
            CounterHudRequest {
                style: counter_style(),
                segments: &segments,
                current_beat: 12.0,
                current_display_beat: 12.0,
                current_bpm: 120.0,
                music_rate: 1.0,
                lookahead: 1,
                multiplier: 1.0,
                vertical: false,
                left: false,
                broken_run: false,
                run_timer: false,
                measure_counter_y: Some(100.0),
                subtractive_scoring_y: 200.0,
                playfield_center_x: 320.0,
                field_zoom: 1.0,
                font: "hud-font",
                counter_text,
                timer_text,
            },
        );

        assert_eq!(actors.len(), 2);
        assert_text(
            &actors[0],
            "(4)",
            [448.0, 100.0],
            [0.4, 0.4, 0.4, 1.0],
            0.3,
            0.5,
            TextAlign::Center,
        );
        assert_text(
            &actors[1],
            "4/8",
            [320.0, 100.0],
            [1.0, 1.0, 1.0, 1.0],
            0.35,
            0.5,
            TextAlign::Center,
        );
    }

    #[test]
    fn run_timer_actor_fingerprint_uses_display_beat_and_active_color() {
        let segments = [StreamSegment {
            start: 0,
            end: 8,
            is_break: false,
        }];
        let mut actors = Vec::new();
        compose_counter_hud(
            &mut actors,
            CounterHudRequest {
                style: counter_style(),
                segments: &segments,
                current_beat: 12.0,
                current_display_beat: 12.0,
                current_bpm: 120.0,
                music_rate: 1.0,
                lookahead: 0,
                multiplier: 1.0,
                vertical: false,
                left: false,
                broken_run: false,
                run_timer: true,
                measure_counter_y: None,
                subtractive_scoring_y: 200.0,
                playfield_center_x: 320.0,
                field_zoom: 1.0,
                font: "hud-font",
                counter_text,
                timer_text,
            },
        );

        assert_eq!(actors.len(), 1);
        assert_text(
            &actors[0],
            " 10",
            [320.0, 200.0],
            [1.0, 1.0, 1.0, 1.0],
            0.35,
            0.5,
            TextAlign::Center,
        );
    }

    #[test]
    fn mini_indicator_actor_fingerprint_preserves_failure_and_anchor() {
        let mut actors = Vec::new();
        compose_mini_indicator(
            &mut actors,
            MiniIndicatorRequest {
                style: MiniIndicatorStyle {
                    column_offset: 1.0,
                    under_up_x_offset: -45.0,
                    unanchored_x_offset: -12.0,
                    failed_color: [0.5, 0.5, 0.5],
                    shadow_len: 1.0,
                    text_z: 85,
                },
                text: Arc::from("-1.23%"),
                color: [1.0, 0.0, 0.0, 0.8],
                failed: true,
                position: LayoutMiniIndicatorPosition::UnderUpArrow,
                counter_left: false,
                playfield_center_x: 320.0,
                field_zoom: 1.0,
                layout_add_x: -10.0,
                y: 200.0,
                zoom: 0.4,
                font: "hud-font",
            },
        );

        assert_eq!(actors.len(), 1);
        assert_text(
            &actors[0],
            "-1.23%",
            [317.0, 200.0],
            [0.5, 0.5, 0.5, 0.8],
            0.4,
            0.0,
            TextAlign::Left,
        );
    }
}
