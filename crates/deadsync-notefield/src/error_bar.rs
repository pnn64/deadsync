use crate::{average_error_bar_mini_scale, smoothstep01};
use deadlib_present::actors::Actor;
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadsync_gameplay::{ErrorBarText, ErrorBarTick, OffsetIndicatorText};
use deadsync_rules::judgment::TimingWindow;
use deadsync_theme::{ErrorBarLayers, ErrorBarPalette, ErrorBarStyle};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ErrorBarModes {
    pub colorful: bool,
    pub monochrome: bool,
    pub highlight: bool,
    pub average: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarState<'a> {
    pub mono_ticks: &'a [Option<ErrorBarTick>],
    pub color_ticks: &'a [Option<ErrorBarTick>],
    pub average_ticks: &'a [Option<ErrorBarTick>],
    pub color_bar_started_at: Option<f32>,
    pub average_bar_started_at: Option<f32>,
    pub flash_early: &'a [Option<f32>],
    pub flash_late: &'a [Option<f32>],
}

#[derive(Clone, Copy, Debug)]
pub struct ErrorBarComposeRequest<'a> {
    pub style: ErrorBarStyle,
    pub modes: ErrorBarModes,
    pub state: ErrorBarState<'a>,
    pub visible: bool,
    pub elapsed_s: f32,
    pub position: [f32; 2],
    pub average_y: f32,
    pub max_height: f32,
    pub mini: f32,
    pub timing_windows_s: [f32; 5],
    pub blue_fantastic_window_s: Option<f32>,
    pub max_window_ix: usize,
    pub show_fa_plus: bool,
    pub judgment_back: bool,
    pub monochrome_background: bool,
    pub multi_tick: bool,
    pub short_average: bool,
    pub center_tick: bool,
    pub has_error_bar: bool,
    pub offset_indicator: Option<OffsetIndicatorText>,
    pub offset_indicator_visible: bool,
    pub offset_indicator_position: [f32; 2],
    pub offset_text: fn(f32) -> Arc<str>,
    pub long_average_tick: Option<ErrorBarTick>,
    pub long_average_visible: bool,
    pub long_average_intensity: f32,
    pub text: Option<ErrorBarText>,
    pub text_visible: bool,
    pub text_label: fn(bool, bool) -> Arc<str>,
}

/// Compose the complete canonical error-bar actor sequence: offset indicator,
/// graphical modes, long-average marker, then textual feedback.
pub fn compose_error_bar(actors: &mut Vec<Actor>, request: ErrorBarComposeRequest<'_>) {
    append_offset_indicator(actors, &request);
    compose_error_bar_modes(actors, request);
    append_long_average(actors, &request);
    append_text_feedback(actors, &request);
}

/// Compose the four canonical graphical error-bar modes in their established
/// insertion order. Theme code supplies only resolved options and visual data.
pub(crate) fn compose_error_bar_modes(
    actors: &mut Vec<Actor>,
    request: ErrorBarComposeRequest<'_>,
) {
    if !request.visible {
        return;
    }
    if request.modes.colorful {
        append_colorful(actors, &request, false);
    }
    if request.modes.monochrome {
        append_monochrome(actors, &request);
    }
    if request.modes.highlight {
        append_colorful(actors, &request, true);
    }
    if request.modes.average {
        append_average(actors, &request);
    }
}

fn append_monochrome(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>) {
    let style = request.style;
    let layers = layers(request);
    let wscale = width_scale(style.monochrome_width, request, 1.0);
    let (bounds_s, bounds_len) = boundaries(request);

    if request.monochrome_background && style.monochrome_background_alpha > 0.0 {
        append_quad(
            actors,
            request.position,
            [
                style.monochrome_width + style.monochrome_border_size,
                request.max_height + style.monochrome_border_size,
            ],
            with_alpha(style.background_color, style.monochrome_background_alpha),
            layers.background,
        );
    }
    append_quad(
        actors,
        request.position,
        [style.monochrome_center_width, request.max_height],
        style.monochrome_center_color,
        layers.band,
    );

    let line_alpha = monochrome_line_alpha(style, request.elapsed_s);
    if line_alpha > 0.0 && wscale.is_finite() && wscale > 0.0 {
        for &bound in bounds_s.iter().take(bounds_len) {
            let offset = bound * wscale;
            if !offset.is_finite() {
                continue;
            }
            for direction in [-1.0_f32, 1.0] {
                append_quad(
                    actors,
                    [
                        request.position[0] + direction * offset,
                        request.position[1],
                    ],
                    [style.monochrome_line_width, request.max_height],
                    with_alpha(style.monochrome_line_color, line_alpha),
                    layers.line,
                );
            }
        }
    }

    let label_alpha = monochrome_label_alpha(style, request.elapsed_s);
    if label_alpha > 0.0 {
        let x = style.monochrome_width * style.label_x_ratio;
        append_label(
            actors,
            request,
            style.early_label,
            request.position[0] - x,
            label_alpha,
            layers.text,
        );
        append_label(
            actors,
            request,
            style.late_label,
            request.position[0] + x,
            label_alpha,
            layers.text,
        );
    }

    if !wscale.is_finite() || wscale <= 0.0 {
        return;
    }
    for tick in request.state.mono_ticks.iter().flatten() {
        let alpha = error_bar_tick_alpha(
            request.elapsed_s - tick.started_at,
            style.monochrome_tick_duration,
            request.multi_tick,
        );
        let x = tick.offset_s * wscale;
        if alpha <= 0.0 || !x.is_finite() {
            continue;
        }
        append_quad(
            actors,
            [request.position[0] + x, request.position[1]],
            [style.tick_width, request.max_height],
            with_alpha(
                error_bar_color_for_window(style.palette, tick.window, request.show_fa_plus),
                alpha,
            ),
            layers.tick,
        );
    }
}

fn append_colorful(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>, highlight: bool) {
    let style = request.style;
    let layers = layers(request);
    let wscale = width_scale(style.colorful_width, request, 1.0);
    let (bounds_s, bounds_len) = boundaries(request);
    let bar_visible = request
        .state
        .color_bar_started_at
        .map(|started| (0.0..style.colorful_tick_duration).contains(&(request.elapsed_s - started)))
        .unwrap_or(false);

    if bar_visible && wscale.is_finite() && wscale > 0.0 {
        append_quad(
            actors,
            request.position,
            [
                style.colorful_width + style.colorful_border_size,
                style.colorful_height + style.colorful_border_size,
            ],
            style.background_color,
            layers.background,
        );
        append_color_bands(
            actors,
            request,
            layers,
            wscale,
            &bounds_s[..bounds_len],
            highlight,
        );
    }

    if !wscale.is_finite() || wscale <= 0.0 {
        return;
    }
    for tick in request.state.color_ticks.iter().flatten() {
        let alpha = error_bar_tick_alpha(
            request.elapsed_s - tick.started_at,
            style.colorful_tick_duration,
            request.multi_tick,
        );
        let x = tick.offset_s * wscale;
        if alpha <= 0.0 || !x.is_finite() {
            continue;
        }
        append_quad(
            actors,
            [request.position[0] + x, request.position[1]],
            [
                style.tick_width,
                style.colorful_height + style.colorful_border_size,
            ],
            with_alpha(style.colorful_tick_color, alpha),
            layers.line,
        );
    }
}

fn append_color_bands(
    actors: &mut Vec<Actor>,
    request: &ErrorBarComposeRequest<'_>,
    layers: ErrorBarLayers,
    wscale: f32,
    bounds_s: &[f32],
    highlight: bool,
) {
    let style = request.style;
    let base = usize::from(!request.show_fa_plus);
    let mut last_x = 0.0_f32;
    for (index, &bound) in bounds_s.iter().enumerate() {
        let x = bound * wscale;
        let width = x - last_x;
        if !x.is_finite() || !width.is_finite() || width <= 0.0 {
            last_x = x;
            continue;
        }
        let window_num = base + index;
        let color = error_bar_color_for_window(
            style.palette,
            timing_window_from_num(window_num),
            request.show_fa_plus,
        );
        let (early_alpha, late_alpha) = if highlight {
            let slot = window_num.min(5);
            (
                error_bar_flash_alpha(
                    request.elapsed_s,
                    request.state.flash_early.get(slot).copied().flatten(),
                    style.colorful_tick_duration,
                    style.highlight_inactive_alpha,
                ),
                error_bar_flash_alpha(
                    request.elapsed_s,
                    request.state.flash_late.get(slot).copied().flatten(),
                    style.colorful_tick_duration,
                    style.highlight_inactive_alpha,
                ),
            )
        } else {
            (1.0, 1.0)
        };
        for (center_x, alpha) in [
            (-0.5 * (last_x + x), early_alpha),
            (0.5 * (last_x + x), late_alpha),
        ] {
            append_quad(
                actors,
                [request.position[0] + center_x, request.position[1]],
                [width, style.colorful_height],
                with_alpha(color, alpha),
                layers.band,
            );
        }
        last_x = x;
    }
}

fn append_average(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>) {
    let style = request.style;
    let mini_scale = average_error_bar_mini_scale(request.mini);
    let wscale = width_scale(style.average_width, request, mini_scale);
    let bar_visible = request
        .state
        .average_bar_started_at
        .map(|started| (0.0..style.colorful_tick_duration).contains(&(request.elapsed_s - started)))
        .unwrap_or(false);
    if !request.short_average || !bar_visible || !wscale.is_finite() || wscale <= 0.0 {
        return;
    }
    let tick_height =
        (style.average_height + style.average_tick_padding + style.average_tick_extra_height)
            * mini_scale;
    if request.center_tick {
        append_quad(
            actors,
            [request.position[0], request.average_y],
            [style.center_tick_width, tick_height],
            style.average_center_tick_color,
            style.average_z,
        );
    }
    for tick in request.state.average_ticks.iter().flatten() {
        let alpha = error_bar_tick_alpha(
            request.elapsed_s - tick.started_at,
            style.colorful_tick_duration,
            request.multi_tick,
        );
        // Intensity scaling, clamping, and the single-sample correction are
        // baked into offset_s when gameplay registers the average tick.
        let x = tick.offset_s * wscale;
        if alpha <= 0.0 || !x.is_finite() {
            continue;
        }
        append_quad(
            actors,
            [request.position[0] + x, request.average_y],
            [style.tick_width * mini_scale, tick_height],
            with_alpha(style.colorful_tick_color, alpha),
            style.average_z,
        );
    }
}

fn append_offset_indicator(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>) {
    let Some(indicator) = request
        .offset_indicator_visible
        .then_some(request.offset_indicator)
        .flatten()
    else {
        return;
    };
    let style = request.style;
    let age = request.elapsed_s - indicator.started_at;
    if !(0.0..style.offset_indicator_duration).contains(&age) {
        return;
    }
    let mut y = request.offset_indicator_position[1];
    if request.has_error_bar {
        let min_separation = request.max_height * 0.5 + style.offset_indicator_gap;
        if (y - request.position[1]).abs() < min_separation {
            y = request.position[1] + min_separation;
        }
    }
    let color = error_bar_color_for_window(style.palette, indicator.window, request.show_fa_plus);
    let mut text = TextBuilder::new();
    text.font(style.offset_indicator_font);
    text.settext((request.offset_text)(indicator.offset_ms).into());
    text.align(0.5, 0.5);
    text.xy(request.offset_indicator_position[0], y);
    text.zoom(style.offset_indicator_zoom);
    text.shadowlength(style.offset_indicator_shadow_len);
    text.diffuse(with_alpha(color, 1.0));
    text.z(layers(request).text);
    actors.push(text.build(0));
}

fn append_long_average(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>) {
    let Some(tick) = request
        .long_average_visible
        .then_some(request.long_average_tick)
        .flatten()
    else {
        return;
    };
    let style = request.style;
    let max_offset_s = request.timing_windows_s[request.max_window_ix.min(4)];
    let width = if request.modes.average {
        style.average_width
    } else if request.modes.colorful {
        style.colorful_width
    } else {
        style.monochrome_width
    };
    let mini_scale = if request.modes.average {
        average_error_bar_mini_scale(request.mini)
    } else {
        1.0
    };
    let wscale = if max_offset_s.is_finite() && max_offset_s > 0.0 {
        width * 0.5 * mini_scale / max_offset_s
    } else {
        0.0
    };
    let alpha = error_bar_tick_alpha(
        request.elapsed_s - tick.started_at,
        style.long_average_tick_duration,
        request.multi_tick,
    );
    if alpha <= 0.0 || !wscale.is_finite() || wscale <= 0.0 {
        return;
    }
    let scaled_offset = if max_offset_s.is_finite() && max_offset_s > 0.0 {
        (tick.offset_s * request.long_average_intensity).clamp(-max_offset_s, max_offset_s)
    } else {
        tick.offset_s * request.long_average_intensity
    };
    let x = scaled_offset * wscale;
    if !x.is_finite() {
        return;
    }
    let (y, z) = if request.modes.average {
        (request.average_y, style.average_z)
    } else {
        (request.position[1], layers(request).line)
    };
    let height =
        (style.average_height + style.average_tick_padding + style.long_average_tick_extra_height)
            * mini_scale;
    append_quad(
        actors,
        [request.position[0] + x, y],
        [style.long_average_tick_width, height],
        with_alpha(style.long_average_tick_color, alpha),
        z,
    );
}

fn append_text_feedback(actors: &mut Vec<Actor>, request: &ErrorBarComposeRequest<'_>) {
    let Some(feedback) = request.text_visible.then_some(request.text).flatten() else {
        return;
    };
    let style = request.style;
    let age = request.elapsed_s - feedback.started_at;
    if !(0.0..style.text_duration).contains(&age) {
        return;
    }
    let x = if feedback.early {
        -style.text_x_offset
    } else {
        style.text_x_offset
    };
    let zoom = if feedback.scaled {
        error_bar_text_scalable_zoom(
            feedback.offset_ms.abs(),
            feedback.scale_start_ms,
            request.timing_windows_s[0] * 1000.0,
        )
    } else {
        style.text_zoom
    };
    let color = match (feedback.early, feedback.scaled) {
        (true, true) => style.text_scaled_early_color,
        (true, false) => style.text_early_color,
        (false, true) => style.text_scaled_late_color,
        (false, false) => style.text_late_color,
    };
    let mut text = TextBuilder::new();
    text.font(style.text_font);
    text.settext((request.text_label)(feedback.early, feedback.scaled).into());
    text.align(0.5, 0.5);
    text.xy(request.position[0] + x, request.position[1]);
    text.zoom(zoom);
    text.shadowlength(style.text_shadow_len);
    text.diffuse(color);
    text.z(layers(request).text);
    actors.push(text.build(0));
}

fn boundaries(request: &ErrorBarComposeRequest<'_>) -> ([f32; 6], usize) {
    error_bar_boundaries_s(
        request.timing_windows_s,
        request.blue_fantastic_window_s,
        request.show_fa_plus,
        request.max_window_ix,
    )
}

fn width_scale(width: f32, request: &ErrorBarComposeRequest<'_>, scale: f32) -> f32 {
    let max_offset_s = request.timing_windows_s[request.max_window_ix.min(4)];
    if max_offset_s.is_finite() && max_offset_s > 0.0 {
        width * 0.5 * scale / max_offset_s
    } else {
        0.0
    }
}

fn layers(request: &ErrorBarComposeRequest<'_>) -> ErrorBarLayers {
    if request.judgment_back {
        request.style.back_layers
    } else {
        request.style.front_layers
    }
}

fn monochrome_line_alpha(style: ErrorBarStyle, elapsed_s: f32) -> f32 {
    if elapsed_s < style.lines_fade_start {
        0.0
    } else if elapsed_s < style.lines_fade_start + style.lines_fade_duration {
        style.line_alpha
            * smoothstep01((elapsed_s - style.lines_fade_start) / style.lines_fade_duration)
    } else {
        style.line_alpha
    }
}

fn monochrome_label_alpha(style: ErrorBarStyle, elapsed_s: f32) -> f32 {
    let fade_out_start = style.label_fade_duration + style.label_hold;
    if elapsed_s < style.label_fade_duration {
        smoothstep01(elapsed_s / style.label_fade_duration)
    } else if elapsed_s < fade_out_start {
        1.0
    } else if elapsed_s < fade_out_start + style.label_fade_duration {
        1.0 - smoothstep01((elapsed_s - fade_out_start) / style.label_fade_duration)
    } else {
        0.0
    }
}

fn append_quad(
    actors: &mut Vec<Actor>,
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    z: i16,
) {
    let mut quad = SpriteBuilder::solid();
    quad.align(0.5, 0.5);
    quad.xy(position[0], position[1]);
    quad.zoomx(size[0]);
    quad.zoomy(size[1]);
    quad.diffuse(color);
    quad.z(z);
    actors.push(quad.build(0));
}

fn append_label(
    actors: &mut Vec<Actor>,
    request: &ErrorBarComposeRequest<'_>,
    label: &'static str,
    x: f32,
    alpha: f32,
    z: i16,
) {
    let style = request.style;
    let mut text = TextBuilder::new();
    text.font(style.label_font);
    text.settext(label.into());
    text.align(0.5, 0.5);
    text.xy(x, request.position[1]);
    text.zoom(style.label_zoom);
    text.diffuse(with_alpha(style.label_color, alpha));
    text.z(z);
    actors.push(text.build(0));
}

const fn with_alpha(mut color: [f32; 4], alpha: f32) -> [f32; 4] {
    color[3] = alpha;
    color
}

pub(crate) fn error_bar_tick_alpha(age: f32, dur: f32, multi_tick: bool) -> f32 {
    if !age.is_finite() || age < 0.0 {
        return 0.0;
    }
    if multi_tick {
        if age < 0.03 {
            1.0
        } else if age < dur {
            1.0 - (age - 0.03) / (dur - 0.03).max(0.000_001)
        } else {
            0.0
        }
    } else if age < dur {
        1.0
    } else {
        0.0
    }
}

pub(crate) fn error_bar_flash_alpha(
    now: f32,
    started_at: Option<f32>,
    dur: f32,
    inactive_alpha: f32,
) -> f32 {
    let Some(started) = started_at else {
        return inactive_alpha;
    };
    let age = now - started;
    if !age.is_finite() || age < 0.0 || age >= dur {
        return inactive_alpha;
    }
    1.0 + (inactive_alpha - 1.0) * (age / dur)
}

pub fn error_bar_boundaries_s(
    windows: [f32; 5],
    fa_plus_s: Option<f32>,
    show_fa_plus: bool,
    max_window_ix: usize,
) -> ([f32; 6], usize) {
    let mut out = [0.0; 6];
    let mut len = 0;
    if show_fa_plus {
        if let Some(v) = fa_plus_s.filter(|v| v.is_finite() && *v > 0.0) {
            out[len] = v;
            len += 1;
        }
    }
    let max = max_window_ix.min(4);
    for w in windows.iter().take(max + 1).copied() {
        out[len] = w;
        len += 1;
    }
    (out, len)
}

pub(crate) const fn timing_window_from_num(n: usize) -> TimingWindow {
    match n {
        0 => TimingWindow::W0,
        1 => TimingWindow::W1,
        2 => TimingWindow::W2,
        3 => TimingWindow::W3,
        4 => TimingWindow::W4,
        _ => TimingWindow::W5,
    }
}

pub(crate) const fn error_bar_color_for_window(
    palette: ErrorBarPalette,
    window: TimingWindow,
    white_w0: bool,
) -> [f32; 4] {
    match window {
        TimingWindow::W0 => palette.fantastic_blue,
        TimingWindow::W1 => {
            if white_w0 {
                palette.fa_plus_white
            } else {
                palette.fantastic_blue
            }
        }
        TimingWindow::W2 => palette.excellent,
        TimingWindow::W3 => palette.great,
        TimingWindow::W4 => palette.decent,
        TimingWindow::W5 => palette.way_off,
    }
}

pub fn error_bar_text_scalable_zoom(abs_ms: f32, scale_start_ms: f32, w2_ms: f32) -> f32 {
    let ms = if abs_ms.is_finite() {
        abs_ms
    } else {
        deadsync_rules::timing::FA_PLUS_W010_MS
    };
    let scale_start_ms = if scale_start_ms.is_finite() && scale_start_ms > 0.0 {
        scale_start_ms
    } else {
        deadsync_rules::timing::FA_PLUS_W010_MS
    };
    let w1_ms = scale_start_ms
        + (deadsync_rules::timing::FA_PLUS_W0_MS - deadsync_rules::timing::FA_PLUS_W010_MS)
            .max(0.001);
    let w2_ms = if w2_ms.is_finite() && w2_ms > w1_ms {
        w2_ms
    } else {
        w1_ms
    };
    let mut scale1 = 1.0;
    let mut scale2 = 1.0;
    if scale_start_ms < ms && ms <= w1_ms {
        scale1 = (ms - scale_start_ms) / (w1_ms - scale_start_ms);
    } else if w1_ms < ms && ms <= w2_ms && w2_ms > w1_ms {
        scale2 = (ms - w1_ms) / (w2_ms - w1_ms);
    }
    0.15 + scale1 * 0.2 + scale2 * 0.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_present::actors::{SizeSpec, SpriteSource};

    const NO_TICKS: [Option<ErrorBarTick>; 0] = [];
    const NO_FLASHES: [Option<f32>; 6] = [None; 6];

    fn style() -> ErrorBarStyle {
        ErrorBarStyle {
            colorful_width: 160.0,
            colorful_height: 10.0,
            colorful_border_size: 4.0,
            average_width: 325.0,
            average_height: 7.0,
            average_tick_padding: 4.0,
            monochrome_width: 240.0,
            monochrome_border_size: 2.0,
            monochrome_center_width: 2.0,
            monochrome_line_width: 1.0,
            tick_width: 2.0,
            colorful_tick_duration: 0.5,
            monochrome_tick_duration: 0.75,
            average_tick_extra_height: 75.0,
            monochrome_background_alpha: 0.5,
            line_alpha: 0.3,
            lines_fade_start: 2.5,
            lines_fade_duration: 0.5,
            label_fade_duration: 0.5,
            label_hold: 2.0,
            label_x_ratio: 0.25,
            label_zoom: 0.7,
            center_tick_width: 1.0,
            highlight_inactive_alpha: 0.3,
            offset_indicator_duration: 0.5,
            offset_indicator_gap: 6.0,
            offset_indicator_zoom: 0.25,
            offset_indicator_shadow_len: 1.0,
            long_average_tick_duration: 0.5,
            long_average_tick_extra_height: 65.0,
            long_average_tick_width: 1.0,
            text_duration: 0.5,
            text_x_offset: 40.0,
            text_zoom: 0.25,
            text_shadow_len: 1.0,
            background_color: [0.0, 0.0, 0.0, 1.0],
            monochrome_center_color: [0.5, 0.5, 0.5, 1.0],
            monochrome_line_color: [1.0, 1.0, 1.0, 1.0],
            label_color: [1.0, 1.0, 1.0, 1.0],
            colorful_tick_color: [0.698, 0.0, 0.0, 1.0],
            average_center_tick_color: [1.0, 1.0, 1.0, 0.3],
            long_average_tick_color: [0.0, 0.0, 1.0, 1.0],
            text_early_color: [0.024, 0.416, 0.957, 1.0],
            text_late_color: [1.0, 0.353, 0.306, 1.0],
            text_scaled_early_color: [0.0, 0.318, 0.859, 1.0],
            text_scaled_late_color: [1.0, 0.086, 0.02, 1.0],
            palette: ErrorBarPalette {
                fantastic_blue: [0.129, 0.8, 0.91, 1.0],
                fa_plus_white: [1.0, 1.0, 1.0, 1.0],
                excellent: [0.886, 0.612, 0.094, 1.0],
                great: [0.4, 0.788, 0.333, 1.0],
                decent: [0.706, 0.361, 1.0, 1.0],
                way_off: [0.788, 0.522, 0.369, 1.0],
            },
            label_font: "game",
            offset_indicator_font: "wendy",
            text_font: "wendy",
            early_label: "Early",
            late_label: "Late",
            front_layers: ErrorBarLayers {
                background: 180,
                band: 181,
                line: 182,
                tick: 183,
                text: 184,
            },
            back_layers: ErrorBarLayers {
                background: 86,
                band: 87,
                line: 88,
                tick: 89,
                text: 90,
            },
            average_z: 88,
        }
    }

    fn empty_state() -> ErrorBarState<'static> {
        ErrorBarState {
            mono_ticks: &NO_TICKS,
            color_ticks: &NO_TICKS,
            average_ticks: &NO_TICKS,
            color_bar_started_at: None,
            average_bar_started_at: None,
            flash_early: &NO_FLASHES,
            flash_late: &NO_FLASHES,
        }
    }

    fn offset_text(offset_ms: f32) -> Arc<str> {
        Arc::from(format!("{offset_ms:.1}"))
    }

    fn text_label(early: bool, scaled: bool) -> Arc<str> {
        Arc::from(match (early, scaled) {
            (true, true) => "Fast",
            (true, false) => "Early",
            (false, true) => "Slow",
            (false, false) => "Late",
        })
    }

    fn request<'a>(modes: ErrorBarModes, state: ErrorBarState<'a>) -> ErrorBarComposeRequest<'a> {
        ErrorBarComposeRequest {
            style: style(),
            modes,
            state,
            visible: true,
            elapsed_s: 1.0,
            position: [100.0, 200.0],
            average_y: 150.0,
            max_height: 30.0,
            mini: 0.0,
            timing_windows_s: [0.02, 0.04, 0.08, 0.12, 0.16],
            blue_fantastic_window_s: None,
            max_window_ix: 0,
            show_fa_plus: false,
            judgment_back: false,
            monochrome_background: true,
            multi_tick: false,
            short_average: true,
            center_tick: true,
            has_error_bar: true,
            offset_indicator: None,
            offset_indicator_visible: false,
            offset_indicator_position: [100.0, 150.0],
            offset_text,
            long_average_tick: None,
            long_average_visible: false,
            long_average_intensity: 1.0,
            text: None,
            text_visible: false,
            text_label,
        }
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-5,
            "expected {expected}, got {actual}"
        );
    }

    fn assert_color(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_close(actual, expected);
        }
    }

    fn assert_quad(actor: &Actor, position: [f32; 2], scale: [f32; 2], tint: [f32; 4], z: i16) {
        match actor {
            Actor::Sprite {
                align,
                offset,
                size,
                source,
                tint: actual_tint,
                z: actual_z,
                scale: actual_scale,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, position);
                assert!(matches!(size, [SizeSpec::Px(0.0), SizeSpec::Px(0.0)]));
                assert!(matches!(source, SpriteSource::Solid));
                assert_close(actual_scale[0], scale[0]);
                assert_close(actual_scale[1], scale[1]);
                assert_color(*actual_tint, tint);
                assert_eq!(*actual_z, z);
            }
            other => panic!("expected error-bar quad, got {other:?}"),
        }
    }

    fn assert_label(actor: &Actor, content: &str, position: [f32; 2], alpha: f32, z: i16) {
        match actor {
            Actor::Text {
                align,
                offset,
                color,
                font,
                content: actual_content,
                z: actual_z,
                scale,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, position);
                assert_color(*color, [1.0, 1.0, 1.0, alpha]);
                assert_eq!(*font, "game");
                assert_eq!(actual_content.as_str(), content);
                assert_eq!(*actual_z, z);
                assert_eq!(*scale, [0.7, 0.7]);
            }
            other => panic!("expected error-bar label, got {other:?}"),
        }
    }

    fn assert_text_actor(
        actor: &Actor,
        font_name: &'static str,
        content: &str,
        position: [f32; 2],
        zoom: f32,
        color: [f32; 4],
        shadow: f32,
        z: i16,
    ) {
        match actor {
            Actor::Text {
                align,
                offset,
                color: actual_color,
                font,
                content: actual_content,
                z: actual_z,
                scale,
                shadow_len,
                ..
            } => {
                assert_eq!(*align, [0.5, 0.5]);
                assert_eq!(*offset, position);
                assert_color(*actual_color, color);
                assert_eq!(*font, font_name);
                assert_eq!(actual_content.as_str(), content);
                assert_eq!(*actual_z, z);
                assert_close(scale[0], zoom);
                assert_close(scale[1], zoom);
                assert_close(shadow_len[0], shadow);
                assert_close(shadow_len[1], -shadow);
            }
            other => panic!("expected error-bar text, got {other:?}"),
        }
    }

    #[test]
    fn hidden_error_bar_emits_nothing() {
        let modes = ErrorBarModes {
            colorful: true,
            monochrome: true,
            highlight: true,
            average: true,
        };
        let mut request = request(modes, empty_state());
        request.visible = false;
        let mut actors = Vec::new();

        compose_error_bar_modes(&mut actors, request);

        assert!(actors.is_empty());
    }

    #[test]
    fn monochrome_actor_fingerprint_preserves_full_order() {
        let ticks = [Some(ErrorBarTick {
            started_at: 2.5,
            offset_s: 0.01,
            window: TimingWindow::W2,
        })];
        let state = ErrorBarState {
            mono_ticks: &ticks,
            ..empty_state()
        };
        let mut request = request(
            ErrorBarModes {
                monochrome: true,
                ..ErrorBarModes::default()
            },
            state,
        );
        request.elapsed_s = 2.75;
        let mut actors = Vec::new();

        compose_error_bar_modes(&mut actors, request);

        assert_eq!(actors.len(), 7);
        assert_quad(
            &actors[0],
            [100.0, 200.0],
            [242.0, 32.0],
            [0.0, 0.0, 0.0, 0.5],
            180,
        );
        assert_quad(
            &actors[1],
            [100.0, 200.0],
            [2.0, 30.0],
            [0.5, 0.5, 0.5, 1.0],
            181,
        );
        assert_quad(
            &actors[2],
            [-20.0, 200.0],
            [1.0, 30.0],
            [1.0, 1.0, 1.0, 0.15],
            182,
        );
        assert_quad(
            &actors[3],
            [220.0, 200.0],
            [1.0, 30.0],
            [1.0, 1.0, 1.0, 0.15],
            182,
        );
        assert_label(&actors[4], "Early", [40.0, 200.0], 0.5, 184);
        assert_label(&actors[5], "Late", [160.0, 200.0], 0.5, 184);
        assert_quad(
            &actors[6],
            [160.0, 200.0],
            [2.0, 30.0],
            [0.886, 0.612, 0.094, 1.0],
            183,
        );
    }

    #[test]
    fn colorful_actor_fingerprint_preserves_bands_and_tick_geometry() {
        let ticks = [Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.01,
            window: TimingWindow::W3,
        })];
        let state = ErrorBarState {
            color_ticks: &ticks,
            color_bar_started_at: Some(0.8),
            ..empty_state()
        };
        let mut actors = Vec::new();

        compose_error_bar_modes(
            &mut actors,
            request(
                ErrorBarModes {
                    colorful: true,
                    ..ErrorBarModes::default()
                },
                state,
            ),
        );

        assert_eq!(actors.len(), 4);
        assert_quad(
            &actors[0],
            [100.0, 200.0],
            [164.0, 14.0],
            [0.0, 0.0, 0.0, 1.0],
            180,
        );
        assert_quad(
            &actors[1],
            [60.0, 200.0],
            [80.0, 10.0],
            [0.129, 0.8, 0.91, 1.0],
            181,
        );
        assert_quad(
            &actors[2],
            [140.0, 200.0],
            [80.0, 10.0],
            [0.129, 0.8, 0.91, 1.0],
            181,
        );
        assert_quad(
            &actors[3],
            [140.0, 200.0],
            [2.0, 14.0],
            [0.698, 0.0, 0.0, 1.0],
            182,
        );
    }

    #[test]
    fn highlight_bands_keep_independent_early_and_late_alpha() {
        let flash_early = [None, Some(1.0), None, None, None, None];
        let flash_late = [None, Some(0.75), None, None, None, None];
        let state = ErrorBarState {
            color_bar_started_at: Some(0.8),
            flash_early: &flash_early,
            flash_late: &flash_late,
            ..empty_state()
        };
        let mut actors = Vec::new();

        compose_error_bar_modes(
            &mut actors,
            request(
                ErrorBarModes {
                    highlight: true,
                    ..ErrorBarModes::default()
                },
                state,
            ),
        );

        assert_eq!(actors.len(), 3);
        assert_quad(
            &actors[1],
            [60.0, 200.0],
            [80.0, 10.0],
            [0.129, 0.8, 0.91, 1.0],
            181,
        );
        assert_quad(
            &actors[2],
            [140.0, 200.0],
            [80.0, 10.0],
            [0.129, 0.8, 0.91, 0.65],
            181,
        );
    }

    #[test]
    fn average_actor_fingerprint_preserves_center_and_tick_geometry() {
        let ticks = [Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.01,
            window: TimingWindow::W1,
        })];
        let state = ErrorBarState {
            average_ticks: &ticks,
            average_bar_started_at: Some(0.8),
            ..empty_state()
        };
        let mut actors = Vec::new();

        compose_error_bar_modes(
            &mut actors,
            request(
                ErrorBarModes {
                    average: true,
                    ..ErrorBarModes::default()
                },
                state,
            ),
        );

        assert_eq!(actors.len(), 2);
        assert_quad(
            &actors[0],
            [100.0, 150.0],
            [1.0, 94.6],
            [1.0, 1.0, 1.0, 0.3],
            88,
        );
        assert_quad(
            &actors[1],
            [189.375, 150.0],
            [2.2, 94.6],
            [0.698, 0.0, 0.0, 1.0],
            88,
        );
    }

    #[test]
    fn combined_modes_preserve_color_mono_highlight_average_order() {
        let mono_ticks = [Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.01,
            window: TimingWindow::W2,
        })];
        let color_ticks = [Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.01,
            window: TimingWindow::W3,
        })];
        let average_ticks = [Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.01,
            window: TimingWindow::W1,
        })];
        let state = ErrorBarState {
            mono_ticks: &mono_ticks,
            color_ticks: &color_ticks,
            average_ticks: &average_ticks,
            color_bar_started_at: Some(0.8),
            average_bar_started_at: Some(0.8),
            ..empty_state()
        };
        let mut actors = Vec::new();

        compose_error_bar_modes(
            &mut actors,
            request(
                ErrorBarModes {
                    colorful: true,
                    monochrome: true,
                    highlight: true,
                    average: true,
                },
                state,
            ),
        );

        assert_eq!(actors.len(), 15);
        assert_quad(
            &actors[0],
            [100.0, 200.0],
            [164.0, 14.0],
            [0.0, 0.0, 0.0, 1.0],
            180,
        );
        assert_quad(
            &actors[4],
            [100.0, 200.0],
            [242.0, 32.0],
            [0.0, 0.0, 0.0, 0.5],
            180,
        );
        assert_quad(
            &actors[9],
            [100.0, 200.0],
            [164.0, 14.0],
            [0.0, 0.0, 0.0, 1.0],
            180,
        );
        assert_quad(
            &actors[13],
            [100.0, 150.0],
            [1.0, 94.6],
            [1.0, 1.0, 1.0, 0.3],
            88,
        );
    }

    #[test]
    fn offset_indicator_preserves_format_color_layer_and_collision_policy() {
        let mut request = request(ErrorBarModes::default(), empty_state());
        request.visible = false;
        request.judgment_back = true;
        request.offset_indicator_visible = true;
        request.offset_indicator = Some(OffsetIndicatorText {
            started_at: 0.8,
            offset_ms: 12.34,
            window: TimingWindow::W2,
        });
        request.offset_indicator_position = [100.0, 195.0];
        let mut actors = Vec::new();

        compose_error_bar(&mut actors, request);

        assert_eq!(actors.len(), 1);
        assert_text_actor(
            &actors[0],
            "wendy",
            "12.3",
            [100.0, 221.0],
            0.25,
            [0.886, 0.612, 0.094, 1.0],
            1.0,
            90,
        );

        request.has_error_bar = false;
        actors.clear();
        compose_error_bar(&mut actors, request);

        assert_eq!(actors.len(), 1);
        assert_text_actor(
            &actors[0],
            "wendy",
            "12.3",
            [100.0, 195.0],
            0.25,
            [0.886, 0.612, 0.094, 1.0],
            1.0,
            90,
        );
    }

    #[test]
    fn long_average_preserves_width_priority_clamp_and_mode_geometry() {
        let tick = ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.03,
            window: TimingWindow::W2,
        };
        let cases = [
            (
                ErrorBarModes {
                    colorful: true,
                    average: true,
                    ..ErrorBarModes::default()
                },
                0.0,
                [278.75, 150.0],
                [1.0, 83.6],
                88,
            ),
            (
                ErrorBarModes {
                    colorful: true,
                    ..ErrorBarModes::default()
                },
                0.0,
                [180.0, 200.0],
                [1.0, 76.0],
                182,
            ),
            (
                ErrorBarModes::default(),
                0.0,
                [220.0, 200.0],
                [1.0, 76.0],
                182,
            ),
        ];

        for (modes, mini, position, scale, z) in cases {
            let mut request = request(modes, empty_state());
            request.visible = false;
            request.mini = mini;
            request.long_average_tick = Some(tick);
            request.long_average_visible = true;
            request.long_average_intensity = 2.0;
            let mut actors = Vec::new();

            compose_error_bar(&mut actors, request);

            assert_eq!(actors.len(), 1);
            assert_quad(&actors[0], position, scale, [0.0, 0.0, 1.0, 1.0], z);
        }
    }

    #[test]
    fn text_feedback_preserves_normal_and_scaled_early_late_fingerprints() {
        let cases = [
            (
                true,
                false,
                "Early",
                [60.0, 200.0],
                0.25,
                [0.024, 0.416, 0.957, 1.0],
            ),
            (
                false,
                false,
                "Late",
                [140.0, 200.0],
                0.25,
                [1.0, 0.353, 0.306, 1.0],
            ),
            (
                true,
                true,
                "Fast",
                [60.0, 200.0],
                0.35,
                [0.0, 0.318, 0.859, 1.0],
            ),
            (
                false,
                true,
                "Slow",
                [140.0, 200.0],
                0.35,
                [1.0, 0.086, 0.02, 1.0],
            ),
        ];

        for (early, scaled, content, position, zoom, color) in cases {
            let mut request = request(ErrorBarModes::default(), empty_state());
            request.visible = false;
            request.text_visible = true;
            request.text = Some(ErrorBarText {
                started_at: 0.8,
                early,
                offset_ms: 12.5,
                scaled,
                scale_start_ms: 10.0,
            });
            let mut actors = Vec::new();

            compose_error_bar(&mut actors, request);

            assert_eq!(actors.len(), 1);
            assert_text_actor(
                &actors[0], "wendy", content, position, zoom, color, 1.0, 184,
            );
        }
    }

    #[test]
    fn full_composer_orders_offset_modes_long_average_then_text() {
        let mut request = request(
            ErrorBarModes {
                monochrome: true,
                ..ErrorBarModes::default()
            },
            empty_state(),
        );
        request.offset_indicator_visible = true;
        request.offset_indicator = Some(OffsetIndicatorText {
            started_at: 0.8,
            offset_ms: 5.0,
            window: TimingWindow::W2,
        });
        request.offset_indicator_position = [100.0, 150.0];
        request.long_average_visible = true;
        request.long_average_tick = Some(ErrorBarTick {
            started_at: 0.8,
            offset_s: 0.005,
            window: TimingWindow::W2,
        });
        request.text_visible = true;
        request.text = Some(ErrorBarText {
            started_at: 0.8,
            early: false,
            offset_ms: 12.5,
            scaled: true,
            scale_start_ms: 10.0,
        });
        let mut actors = Vec::new();

        compose_error_bar(&mut actors, request);

        assert_eq!(actors.len(), 7);
        assert_text_actor(
            &actors[0],
            "wendy",
            "5.0",
            [100.0, 150.0],
            0.25,
            [0.886, 0.612, 0.094, 1.0],
            1.0,
            184,
        );
        assert_quad(
            &actors[1],
            [100.0, 200.0],
            [242.0, 32.0],
            [0.0, 0.0, 0.0, 0.5],
            180,
        );
        assert_quad(
            &actors[2],
            [100.0, 200.0],
            [2.0, 30.0],
            [0.5, 0.5, 0.5, 1.0],
            181,
        );
        assert_label(&actors[3], "Early", [40.0, 200.0], 1.0, 184);
        assert_label(&actors[4], "Late", [160.0, 200.0], 1.0, 184);
        assert_quad(
            &actors[5],
            [130.0, 200.0],
            [1.0, 76.0],
            [0.0, 0.0, 1.0, 1.0],
            182,
        );
        assert_text_actor(
            &actors[6],
            "wendy",
            "Slow",
            [140.0, 200.0],
            0.35,
            [1.0, 0.086, 0.02, 1.0],
            1.0,
            184,
        );
    }
}
