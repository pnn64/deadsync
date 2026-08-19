use crate::{
    BrokenRunLookup, LayoutMiniIndicatorPosition, ZmodMeasureCounterText,
    zmod_broken_run_counter_text, zmod_measure_counter_text,
};
use deadlib_present::actors::{
    Actor, FlatDraw, FlatPreparedInline, InlineText, TextAlign, TextContent,
};
use deadlib_present::dsl::TextBuilder;
use deadlib_render_core::BlendMode;
use deadsync_rules::scroll::ScrollSpeedSetting;
use deadsync_rules::stream::StreamSegment;
use deadsync_theme::{CounterHudStyle, MiniIndicatorStyle};

pub const MEASURE_COUNTER_LOOKAHEAD_MAX: u8 = 4;
pub const COUNTER_TEXT_SLOTS_PER_PLAYER: u8 = MEASURE_COUNTER_LOOKAHEAD_MAX + 3;
const BROKEN_COUNTER_TEXT_SLOT: u8 = MEASURE_COUNTER_LOOKAHEAD_MAX + 1;
const RUN_TIMER_TEXT_SLOT: u8 = MEASURE_COUNTER_LOOKAHEAD_MAX + 2;

#[derive(Clone, Copy)]
pub(crate) struct CounterHudRequest<'a> {
    pub style: CounterHudStyle,
    pub segments: &'a [StreamSegment],
    pub broken_run_lookup: &'a BrokenRunLookup,
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
    pub frame_text_slot: u8,
    pub counter_text: fn(ZmodMeasureCounterText) -> TextContent,
    pub timer_text: fn(i32, i32, bool) -> TextContent,
}

/// Compose the canonical measure counter, broken-run counter, and run timer.
/// The caller supplies resolved gameplay values, theme metrics, and cached text
/// formatters; placement and actor construction stay inside the notefield.
pub(crate) fn compose_counter_hud(
    actors: &mut Vec<Actor>,
    draws: &mut Vec<FlatDraw>,
    request: CounterHudRequest<'_>,
) {
    let segments = request.segments;
    if segments.is_empty() {
        return;
    }

    let mut plan = CounterHudTextPlan::default();

    let beat_floor = request.current_beat.floor();
    let current_measure = beat_floor / 4.0;
    let (base_index, run_timer_index) = request
        .broken_run_lookup
        .segment_indices(segments, current_measure);
    let mut column_width = ScrollSpeedSetting::ARROW_SPACING * request.field_zoom;
    if request.left {
        column_width *= request.style.left_column_scale;
    }

    if let Some(counter_y) = request.measure_counter_y {
        append_measure_counters(
            &mut plan,
            request,
            beat_floor,
            current_measure,
            base_index,
            column_width,
            counter_y,
        );
        append_broken_counter(&mut plan, request, current_measure, column_width, counter_y);
    }
    append_run_timer(&mut plan, request, run_timer_index, column_width);
    emit_counter_hud(actors, draws, request.style, request.font, plan);
}

struct HudTextRun {
    content: TextContent,
    offset: [f32; 2],
    align: [f32; 2],
    align_text: TextAlign,
    zoom: f32,
    color: [f32; 4],
}

#[derive(Default)]
struct CounterHudTextPlan {
    entries: [Option<HudTextRun>; COUNTER_TEXT_SLOTS_PER_PLAYER as usize],
    len: usize,
}

impl CounterHudTextPlan {
    fn push(&mut self, text: HudTextRun) {
        let entry = self
            .entries
            .get_mut(self.len)
            .expect("counter HUD plan stays within its prepared slot envelope");
        *entry = Some(text);
        self.len += 1;
    }
}

#[allow(clippy::too_many_arguments)]
fn append_measure_counters(
    plan: &mut CounterHudTextPlan,
    request: CounterHudRequest<'_>,
    beat_floor: f32,
    current_measure: f32,
    base_index: usize,
    column_width: f32,
    counter_y: f32,
) {
    let lookahead = request.lookahead.min(MEASURE_COUNTER_LOOKAHEAD_MAX);
    for j in (0..=lookahead).rev() {
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
            lookahead.into(),
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
            let denominator = if lookahead == 0 {
                1.0
            } else {
                f32::from(lookahead)
            };
            x += (column_width / denominator) * request.style.horizontal_span * f32::from(j);
        }
        if request.left {
            x -= column_width;
        }
        append_hud_text(
            plan,
            (request.counter_text)(text_kind)
                .with_frame_inline_slot(request.frame_text_slot.saturating_add(j)),
            [x, y],
            [0.5, 0.5],
            zoom,
            color,
        );
    }
}

fn append_broken_counter(
    plan: &mut CounterHudTextPlan,
    request: CounterHudRequest<'_>,
    current_measure: f32,
    column_width: f32,
    counter_y: f32,
) {
    if !request.broken_run {
        return;
    }
    let Some((segment_index, broken_end, is_broken)) =
        request.broken_run_lookup.segment(current_measure)
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
        plan,
        (request.counter_text)(text_kind).with_frame_inline_slot(
            request
                .frame_text_slot
                .saturating_add(BROKEN_COUNTER_TEXT_SLOT),
        ),
        [x, y],
        [0.5, 0.5],
        request.style.base_zoom,
        request.style.broken_color,
    );
}

fn append_run_timer(
    plan: &mut CounterHudTextPlan,
    request: CounterHudRequest<'_>,
    segment_index: usize,
    column_width: f32,
) {
    if !request.run_timer {
        return;
    }
    let Some(segment) = request.segments.get(segment_index).copied() else {
        return;
    };
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
    let color = if text.as_str().contains(' ') {
        request.style.run_active_color
    } else {
        request.style.run_inactive_color
    };
    let mut x = request.playfield_center_x;
    if request.left {
        x -= column_width;
    }
    append_hud_text(
        plan,
        text.with_frame_inline_slot(request.frame_text_slot.saturating_add(RUN_TIMER_TEXT_SLOT)),
        [x, request.subtractive_scoring_y],
        [0.5, 0.5],
        request.style.base_zoom,
        color,
    );
}

fn append_hud_text(
    plan: &mut CounterHudTextPlan,
    content: TextContent,
    offset: [f32; 2],
    align: [f32; 2],
    zoom: f32,
    color: [f32; 4],
) {
    plan.push(HudTextRun {
        content,
        offset,
        align,
        align_text: TextAlign::Center,
        zoom,
        color,
    });
}

fn emit_counter_hud(
    actors: &mut Vec<Actor>,
    draws: &mut Vec<FlatDraw>,
    style: CounterHudStyle,
    font: &'static str,
    plan: CounterHudTextPlan,
) {
    let direct = plan.entries[..plan.len].iter().all(|entry| {
        matches!(
            entry.as_ref().map(|text| &text.content),
            Some(TextContent::FrameInline { .. })
        )
    });
    for entry in plan.entries.into_iter().take(plan.len).flatten() {
        if direct {
            let (text, slot) = match &entry.content {
                TextContent::FrameInline { text, slot } => (*text, *slot),
                _ => unreachable!("direct counter HUD eligibility checked before emission"),
            };
            append_flat_hud_text(
                draws,
                style.text_z,
                style.shadow_len,
                font,
                text,
                slot,
                entry,
            );
        } else {
            actors.push(hud_text_actor(style, font, entry));
        }
    }
}

fn append_flat_hud_text(
    draws: &mut Vec<FlatDraw>,
    text_z: i16,
    shadow_len: f32,
    font: &'static str,
    text: InlineText,
    slot: u8,
    run: HudTextRun,
) {
    draws.push(FlatDraw::PreparedInline(FlatPreparedInline {
        align: run.align,
        offset: run.offset,
        color: run.color,
        font,
        text,
        slot,
        align_text: run.align_text,
        z: text_z,
        scale: [run.zoom, run.zoom],
        blend: BlendMode::Alpha,
        shadow_len: [shadow_len, -shadow_len],
        shadow_color: [0.0, 0.0, 0.0, 0.5],
    }));
}

fn hud_text_actor(style: CounterHudStyle, font: &'static str, run: HudTextRun) -> Actor {
    let mut text = TextBuilder::new();
    text.font(font);
    text.settext(run.content);
    text.align(run.align[0], run.align[1]);
    text.horizalign(run.align_text);
    text.xy(run.offset[0], run.offset[1]);
    text.zoom(run.zoom);
    text.shadowlength(style.shadow_len);
    text.diffuse(run.color);
    text.z(style.text_z);
    text.build(0)
}

pub(crate) struct MiniIndicatorRequest {
    pub style: MiniIndicatorStyle,
    pub text: TextContent,
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
pub(crate) fn compose_mini_indicator(
    actors: &mut Vec<Actor>,
    draws: &mut Vec<FlatDraw>,
    request: MiniIndicatorRequest,
) {
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

    let run = HudTextRun {
        content: request.text,
        offset: [x, request.y],
        align: [align_x, 0.5],
        align_text: TextAlign::Left,
        zoom: request.zoom,
        color,
    };
    if let TextContent::FrameInline { text, slot } = &run.content {
        let (text, slot) = (*text, *slot);
        append_flat_hud_text(
            draws,
            request.style.text_z,
            request.style.shadow_len,
            request.font,
            text,
            slot,
            run,
        );
    } else {
        actors.push(mini_text_actor(request.style, request.font, run));
    }
}

fn mini_text_actor(style: MiniIndicatorStyle, font: &'static str, run: HudTextRun) -> Actor {
    let mut text = TextBuilder::new();
    text.font(font);
    text.settext(run.content);
    text.align(run.align[0], run.align[1]);
    text.horizalign(run.align_text);
    text.xy(run.offset[0], run.offset[1]);
    text.zoom(run.zoom);
    text.shadowlength(style.shadow_len);
    text.diffuse(run.color);
    text.z(style.text_z);
    text.build(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::InlineText;

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

    fn counter_text(value: ZmodMeasureCounterText) -> TextContent {
        let text = match value {
            ZmodMeasureCounterText::Ratio { current, total } => {
                format!("{current}/{total}")
            }
            ZmodMeasureCounterText::Break(value) => format!("({value})"),
            ZmodMeasureCounterText::Total(value) => value.to_string(),
        };
        TextContent::Inline(InlineText::copy_from(&text).expect("test counter text fits inline"))
    }

    fn timer_text(value: i32, _mode: i32, active: bool) -> TextContent {
        let text = if active {
            format!(" {value}")
        } else {
            value.to_string()
        };
        TextContent::Inline(InlineText::copy_from(&text).expect("test timer text fits inline"))
    }

    fn shared_ratio_text(value: ZmodMeasureCounterText) -> TextContent {
        match value {
            ZmodMeasureCounterText::Ratio { current, total } => {
                TextContent::Shared(format!("{current}/{total}").into())
            }
            other => counter_text(other),
        }
    }

    fn assert_actor_text(
        actor: &Actor,
        content: &str,
        offset: [f32; 2],
        color: [f32; 4],
        zoom: f32,
        align_x: f32,
        text_align: TextAlign,
        frame_slot: Option<u8>,
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
                assert_eq!(
                    match actual_content {
                        TextContent::FrameInline { slot, .. } => Some(*slot),
                        _ => None,
                    },
                    frame_slot
                );
                assert_eq!(*align_text, text_align);
                assert_eq!(*z, 85);
                assert!((scale[0] - zoom).abs() <= 1e-6);
                assert!((scale[1] - zoom).abs() <= 1e-6);
                assert_eq!(*shadow_len, [1.0, -1.0]);
            }
            other => panic!("expected HUD text, got {other:?}"),
        }
    }

    fn assert_flat_text(
        draw: &FlatDraw,
        content: &str,
        offset: [f32; 2],
        color: [f32; 4],
        zoom: f32,
        align_x: f32,
        text_align: TextAlign,
        frame_slot: u8,
    ) {
        match draw {
            FlatDraw::PreparedInline(text) => {
                assert_eq!(text.align, [align_x, 0.5]);
                assert_eq!(text.offset, offset);
                assert_eq!(text.color, color);
                assert_eq!(text.font, "hud-font");
                assert_eq!(text.text.as_str(), content);
                assert_eq!(text.slot, frame_slot);
                assert_eq!(text.align_text, text_align);
                assert_eq!(text.z, 85);
                assert!((text.scale[0] - zoom).abs() <= 1e-6);
                assert!((text.scale[1] - zoom).abs() <= 1e-6);
                assert_eq!(text.shadow_len, [1.0, -1.0]);
                assert_eq!(text.shadow_color, [0.0, 0.0, 0.0, 0.5]);
            }
            other => panic!("expected direct HUD text, got {other:?}"),
        }
    }

    #[test]
    fn measure_counter_direct_fingerprint_preserves_order_and_lookahead() {
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
        let broken_run_lookup = BrokenRunLookup::new(&segments);
        let mut actors = Vec::new();
        let mut draws = Vec::new();
        compose_counter_hud(
            &mut actors,
            &mut draws,
            CounterHudRequest {
                style: counter_style(),
                segments: &segments,
                broken_run_lookup: &broken_run_lookup,
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
                frame_text_slot: 20,
                counter_text,
                timer_text,
            },
        );

        assert!(actors.is_empty());
        assert_eq!(draws.len(), 2);
        assert_flat_text(
            &draws[0],
            "(4)",
            [448.0, 100.0],
            [0.4, 0.4, 0.4, 1.0],
            0.3,
            0.5,
            TextAlign::Center,
            21,
        );
        assert_flat_text(
            &draws[1],
            "4/8",
            [320.0, 100.0],
            [1.0, 1.0, 1.0, 1.0],
            0.35,
            0.5,
            TextAlign::Center,
            20,
        );
    }

    #[test]
    fn run_timer_direct_fingerprint_uses_display_beat_and_active_color() {
        let segments = [StreamSegment {
            start: 0,
            end: 8,
            is_break: false,
        }];
        let broken_run_lookup = BrokenRunLookup::new(&segments);
        let mut actors = Vec::new();
        let mut draws = Vec::new();
        compose_counter_hud(
            &mut actors,
            &mut draws,
            CounterHudRequest {
                style: counter_style(),
                segments: &segments,
                broken_run_lookup: &broken_run_lookup,
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
                frame_text_slot: 20,
                counter_text,
                timer_text,
            },
        );

        assert!(actors.is_empty());
        assert_eq!(draws.len(), 1);
        assert_flat_text(
            &draws[0],
            " 10",
            [320.0, 200.0],
            [1.0, 1.0, 1.0, 1.0],
            0.35,
            0.5,
            TextAlign::Center,
            26,
        );
    }

    #[test]
    fn shared_ratio_keeps_the_complete_counter_group_on_actors() {
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
        let broken_run_lookup = BrokenRunLookup::new(&segments);
        let mut actors = Vec::new();
        let mut draws = Vec::new();
        compose_counter_hud(
            &mut actors,
            &mut draws,
            CounterHudRequest {
                style: counter_style(),
                segments: &segments,
                broken_run_lookup: &broken_run_lookup,
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
                frame_text_slot: 20,
                counter_text: shared_ratio_text,
                timer_text,
            },
        );

        assert_eq!(actors.len(), 2);
        assert!(draws.is_empty());
        assert_actor_text(
            &actors[0],
            "(4)",
            [448.0, 100.0],
            [0.4, 0.4, 0.4, 1.0],
            0.3,
            0.5,
            TextAlign::Center,
            Some(21),
        );
        assert_actor_text(
            &actors[1],
            "4/8",
            [320.0, 100.0],
            [1.0, 1.0, 1.0, 1.0],
            0.35,
            0.5,
            TextAlign::Center,
            None,
        );
    }

    #[test]
    fn mini_indicator_actor_fingerprint_preserves_failure_and_anchor() {
        let mut actors = Vec::new();
        let mut draws = Vec::new();
        compose_mini_indicator(
            &mut actors,
            &mut draws,
            MiniIndicatorRequest {
                style: MiniIndicatorStyle {
                    column_offset: 1.0,
                    under_up_x_offset: -45.0,
                    unanchored_x_offset: -12.0,
                    failed_color: [0.5, 0.5, 0.5],
                    shadow_len: 1.0,
                    text_z: 85,
                },
                text: TextContent::Static("-1.23%"),
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
        assert!(draws.is_empty());
        assert_actor_text(
            &actors[0],
            "-1.23%",
            [317.0, 200.0],
            [0.5, 0.5, 0.5, 0.8],
            0.4,
            0.0,
            TextAlign::Left,
            None,
        );
    }

    #[test]
    fn mini_indicator_uses_direct_prepared_inline_text() {
        let mut actors = Vec::new();
        let mut draws = Vec::new();
        compose_mini_indicator(
            &mut actors,
            &mut draws,
            MiniIndicatorRequest {
                style: MiniIndicatorStyle {
                    column_offset: 1.0,
                    under_up_x_offset: -45.0,
                    unanchored_x_offset: -12.0,
                    failed_color: [0.5, 0.5, 0.5],
                    shadow_len: 1.0,
                    text_z: 85,
                },
                text: TextContent::frame_inline_slot(
                    InlineText::copy_from("-1.23%").expect("test indicator fits inline"),
                    11,
                ),
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

        assert!(actors.is_empty());
        assert_eq!(draws.len(), 1);
        assert_flat_text(
            &draws[0],
            "-1.23%",
            [317.0, 200.0],
            [0.5, 0.5, 0.5, 0.8],
            0.4,
            0.0,
            TextAlign::Left,
            11,
        );
    }
}
