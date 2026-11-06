use super::*;
pub fn average(draws: &mut Vec<FlatDraw>, request: &ErrorBarComposeRequest<'_>) {
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
            draws,
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
            draws,
            [request.position[0] + x, request.average_y],
            [style.tick_width * mini_scale, tick_height],
            with_alpha(style.colorful_tick_color, alpha),
            style.average_z,
        );
    }
}
pub fn long(draws: &mut Vec<FlatDraw>, request: &ErrorBarComposeRequest<'_>) {
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
        draws,
        [request.position[0] + x, y],
        [style.long_average_tick_width, height],
        with_alpha(style.long_average_tick_color, alpha),
        z,
    );
}
pub fn modes(draws: &mut Vec<FlatDraw>, request: ErrorBarComposeRequest<'_>) {
    if !request.visible {
        return;
    }
    if request.modes.colorful {
        append_colorful(draws, &request, false);
    }
    if request.modes.monochrome {
        append_monochrome(draws, &request);
    }
    if request.modes.highlight {
        append_colorful(draws, &request, true);
    }
    if request.modes.average {
        average(draws, &request);
    }
}
pub fn compose(draws: &mut Vec<FlatDraw>, request: ErrorBarComposeRequest<'_>) {
    append_offset_indicator(draws, &request);
    modes(draws, request);
    long(draws, &request);
    append_text_feedback(draws, &request);
}

pub fn average_error_bar_mini_scale(mini: f32) -> f32 {
    0.545f32.mul_add(-mini, 1.1).max(0.0)
}
