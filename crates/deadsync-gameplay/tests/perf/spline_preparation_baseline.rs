// Frozen from a85e01991 (0.5.1203); receiver adapted for the test module.
use super::*;
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
        let mut diagonal = vec![4.0_f32; size];
        let mut slopes = vec![0.0; size];
        diagonal[0] = 2.0;
        diagonal[size - 1] = 2.0;
        slopes[0] = 3.0 * (points[1][0] - points[0][0]);
        for i in 1..size - 1 {
            slopes[i] = 3.0 * (points[i + 1][0] - points[i - 1][0]);
        }
        slopes[size - 1] = 3.0 * (points[size - 1][0] - points[size - 2][0]);
        for i in 1..size {
            let multiple = 1.0 / diagonal[i - 1];
            diagonal[i] -= multiple;
            slopes[i] -= slopes[i - 1] * multiple;
        }
        for i in (1..size).rev() {
            slopes[i - 1] -= slopes[i] * (1.0 / diagonal[i]);
        }
        for (slope, diagonal) in slopes.iter_mut().zip(diagonal) {
            *slope /= diagonal;
        }
        for i in 0..size - 1 {
            let diff = points[i + 1][0] - points[i][0];
            points[i][1] = slopes[i];
            points[i][2] = 3.0 * diff - 2.0 * slopes[i] - slopes[i + 1];
            points[i][3] = -2.0 * diff + slopes[i] + slopes[i + 1];
        }
    }
    state.zoom_splines[column] = SongLuaZoomSpline {
        beats_per_t,
        coefficients: points.into_boxed_slice(),
    };
}
