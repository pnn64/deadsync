// Frozen from 7199d99cc (0.5.1170) for behavior comparisons and benchmarks.
use super::*;

pub(super) fn column_transform_windows_from_samples(
    from_samples: &[SongLuaColumnTransformSample],
    to_samples: &[SongLuaColumnTransformSample],
    params: SongLuaColumnOffsetBuildParams,
) -> Vec<SongLuaColumnOffsetWindow> {
    let mut keys = Vec::<(usize, usize, SongLuaColumnTransformTarget)>::new();
    for sample in from_samples.iter().chain(to_samples.iter()) {
        let key = (sample.player, sample.column, sample.target);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys.sort_unstable();

    let mut out = Vec::new();
    for (player, column, target) in keys {
        let from_y = column_transform_sample_value(from_samples, player, column, target);
        let to_y = column_transform_sample_value(to_samples, player, column, target);
        let baseline = target.baseline();
        if (from_y - baseline).abs() <= f32::EPSILON && (to_y - baseline).abs() <= f32::EPSILON {
            continue;
        }
        out.push(SongLuaColumnOffsetWindow {
            unit: params.unit,
            start: params.start,
            limit: params.limit,
            span_mode: params.span_mode,
            player,
            column,
            target,
            from_y,
            to_y,
            easing: params.easing.clone(),
            sustain: params.sustain,
            opt1: params.opt1,
            opt2: params.opt2,
        });
    }
    out
}

pub(super) fn note_column_pos_offset_y(actor: &Table) -> Result<Option<f32>, String> {
    let Some(handler) = actor
        .get::<Option<Table>>("__songlua_pos_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Ok(crate::note_column_pos_offset_y_from_points(&mode, &[]));
    }
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return Ok(None);
    }
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    if size == 0 {
        return Ok(crate::note_column_pos_offset_y_from_points(&mode, &[]));
    }
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut points_out = Vec::with_capacity(size);
    for index in 1..=size {
        let Some(point) = points
            .raw_get::<Option<Table>>(index)
            .map_err(|err| err.to_string())?
        else {
            return Ok(None);
        };
        let x = point.raw_get::<Value>(1).ok().and_then(read_f32);
        let point_y = point.raw_get::<Value>(2).ok().and_then(read_f32);
        let (Some(x), Some(point_y)) = (x, point_y) else {
            return Ok(None);
        };
        points_out.push([x, point_y]);
    }
    Ok(crate::note_column_pos_offset_y_from_points(
        &mode,
        &points_out,
    ))
}

pub(super) fn update_function_replay_beats(
    context: &SongLuaCompileContext,
    start: f32,
    end: f32,
) -> Vec<(f64, f64)> {
    let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
    let end_seconds = f64::from(song_elapsed_seconds_at(end, context));
    let mut out = vec![(f64::from(start), 0.0)];
    let frame_count = ((end_seconds - start_seconds) * f64::from(60.0_f32))
        .ceil()
        .max(0.0) as usize;
    let mut previous_seconds = start_seconds;
    for frame in 1..=frame_count {
        let seconds = (start_seconds + frame as f64 / f64::from(60.0_f32)).min(end_seconds);
        out.push((
            crate::song_beat_at_seconds64(seconds, context),
            seconds - previous_seconds,
        ));
        previous_seconds = seconds;
    }
    out
}
