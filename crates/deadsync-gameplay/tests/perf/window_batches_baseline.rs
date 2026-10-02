// Frozen from parent 34dfff9a7a0da3b27213df59d5ea39e473f63ba3 (0.5.1686).
// Shared scalar cutoff, timing rebuild and spline solve helpers are unchanged.

fn old_set_zoom_spline(
    state: &mut SongLuaNoteHideWindows,
    column: usize,
    beats_per_t: f32,
    size: usize,
) {
    if column >= MAX_COLS
        || !(1..=65_536).contains(&size)
        || !beats_per_t.is_finite()
        || beats_per_t <= 0.0
    {
        return;
    }
    let mut points = vec![[0.0; 4]; size];
    for window in state.column_windows(column) {
        if !window.start_beat.is_finite() || !window.end_beat.is_finite() {
            continue;
        }
        let start = (window.start_beat / beats_per_t).round().max(0.0) as usize;
        let end = (window.end_beat / beats_per_t).round().max(0.0) as usize;
        for point in points.iter_mut().take(end.saturating_add(1)).skip(start) {
            point[0] = -1.0;
        }
    }
    if size == 2 {
        points[0][1] = points[1][0] - points[0][0];
    } else if size > 2 {
        // Until the final coefficient pass, the b/c output slots are
        // scratch for the slope and diagonal. Preserve the floating-point
        // calculations without allocating two extra size-element arrays.
        points[0][2] = 2.0;
        points[0][1] = 3.0 * (points[1][0] - points[0][0]);
        for i in 1..size - 1 {
            let slope = 3.0 * (points[i + 1][0] - points[i - 1][0]);
            let multiple = 1.0 / points[i - 1][2];
            points[i][2] = 4.0 - multiple;
            points[i][1] = slope - points[i - 1][1] * multiple;
        }
        let multiple = 1.0 / points[size - 2][2];
        let mut next_diagonal = 2.0 - multiple;
        let mut next_slope =
            3.0 * (points[size - 1][0] - points[size - 2][0]) - points[size - 2][1] * multiple;
        let mut next_b = next_slope / next_diagonal;
        let mut next_a = points[size - 1][0];
        // The terminal segment has no cubic in the original representation.
        // Its zero-initialized slots can stay untouched. Solve backwards and
        // finish each preceding segment once its neighbor's slope is known.
        for point in points[..size - 1].iter_mut().rev() {
            let slope = point[1] - next_slope * (1.0 / next_diagonal);
            let diagonal = point[2];
            let b = slope / diagonal;
            let diff = next_a - point[0];
            point[1] = b;
            point[2] = 3.0 * diff - 2.0 * b - next_b;
            point[3] = -2.0 * diff + b + next_b;
            next_slope = slope;
            next_diagonal = diagonal;
            next_b = b;
            next_a = point[0];
        }
    }
    state.zoom_splines[column] = SongLuaZoomSpline {
        beats_per_t,
        coefficients: points.into_boxed_slice(),
    };
}

fn old_extend_ease_tails(out: &mut [SongLuaEaseMaskWindow], constants: &[AttackMaskWindow]) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    if out.iter().any(|window| !window.start_second.is_finite()) {
        song_lua_extend_ease_tails_nonfinite(out, constants);
        return;
    }

    with_song_lua_tail_indices(out.len(), |indices| {
        indices.sort_unstable_by(|&left, &right| {
            out[left]
                .target
                .cmp(&out[right].target)
                .then_with(|| out[left].start_second.total_cmp(&out[right].start_second))
                .then_with(|| left.cmp(&right))
        });
        let mut group_start = 0;
        while group_start < indices.len() {
            let target = out[indices[group_start]].target;
            let group_end = indices[group_start..]
                .partition_point(|&index| out[index].target == target)
                + group_start;
            let mut next = group_start + 1;
            for position in group_start..group_end {
                next = next.max(position + 1);
                let index = indices[position];
                let start_second = out[index].start_second;
                while next < group_end
                    && out[indices[next]].start_second <= start_second + SAME_TICK_EPSILON
                {
                    next += 1;
                }

                let window = &out[index];
                let default_end =
                    if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
                        window.sustain_end_second
                    } else {
                        f32::MAX
                    };
                let next_start = (next < group_end).then(|| out[indices[next]].start_second);
                let cutoff_second = constants
                    .iter()
                    .filter_map(|constant| {
                        song_lua_constant_cutoff_second(constant, window, SAME_TICK_EPSILON)
                    })
                    .fold(next_start, |acc, start| {
                        Some(acc.map_or(start, |current| current.min(start)))
                    });
                out[index].sustain_end_second =
                    cutoff_second.map_or(default_end, |cutoff| default_end.min(cutoff));
            }
            group_start = group_end;
        }
    });
}

fn old_update_active<T>(
    state: &mut ActiveWindowIndex,
    windows: &[T],
    now: f32,
    start_second: impl Fn(&T) -> f32 + Copy,
    expiry_second: impl Fn(&T) -> f32 + Copy,
    is_active: impl Fn(&T, f32) -> bool + Copy,
) {
    state.ensure_source(windows, start_second);
    if !now.is_finite()
        || state
            .last_now
            .is_none_or(|last| !last.is_finite() || now < last)
    {
        state.rebuild_time(windows, now, start_second, expiry_second, is_active);
        return;
    }

    while state.next_start_second <= now {
        let index = state.start_order[state.next_start];
        state.next_start += 1;
        state.next_start_second = state
            .start_order
            .get(state.next_start)
            .map_or(f32::INFINITY, |&index| start_second(&windows[index]));
        if is_active(&windows[index], now) {
            if state.active.last().is_none_or(|&last| last < index) {
                state.active.push(index);
            } else {
                let insert_at = state.active.binary_search(&index).unwrap_or_else(|at| at);
                state.active.insert(insert_at, index);
            }
            state.next_expiry_second = state.next_expiry_second.min(expiry_second(&windows[index]));
            state.stats.activations = state.stats.activations.saturating_add(1);
        }
    }
    if state.next_expiry_second <= now {
        let previous_len = state.active.len();
        let mut next_expiry_second = f32::INFINITY;
        state.active.retain(|&index| {
            let active = is_active(&windows[index], now);
            if active {
                next_expiry_second = next_expiry_second.min(expiry_second(&windows[index]));
            }
            active
        });
        state.next_expiry_second = next_expiry_second;
        state.stats.pruned = state
            .stats
            .pruned
            .saturating_add(previous_len.saturating_sub(state.active.len()) as u64);
    }
    state.stats.max_active = state.stats.max_active.max(state.active.len());
    state.last_now = Some(now);
}
