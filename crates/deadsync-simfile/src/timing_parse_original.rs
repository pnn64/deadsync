pub(super) fn parse_time_signatures_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, i32, i32) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 4, 4)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(numerator), Some(denominator)) =
            (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(beat), Ok(numerator), Ok(denominator)) = (
            beat.trim().parse::<f32>(),
            numerator.trim().parse::<i32>(),
            denominator.trim().parse::<i32>(),
        ) else {
            continue;
        };
        if beat.is_finite() && numerator > 0 && denominator > 0 {
            out.push(make(beat, numerator, denominator));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 4, 4)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    out.dedup_by(|a, b| beat_to_note_row(beat(a)) == beat_to_note_row(beat(b)));
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 4, 4));
    }
    out
}

pub(super) fn parse_tickcounts_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, u8) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 4)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(ticks)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(ticks)) = (beat.trim().parse::<f32>(), parse_itg_int(ticks)) else {
            continue;
        };
        if beat.is_finite() {
            out.push(make(beat, ticks.clamp(0, 48) as u8));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 4)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    dedup_last_by_row(&mut out, &beat);
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 4));
    }
    out
}

pub(super) fn parse_combos_as<T>(
    tag: Option<&str>,
    make: impl Fn(f32, u32, u32) -> T,
    beat: impl Fn(&T) -> f32,
) -> Vec<T> {
    let Some(s) = tag.map(str::trim).filter(|s| !s.is_empty()) else {
        return vec![make(0.0, 1, 1)];
    };

    let mut out = Vec::with_capacity(timing_segment_capacity(s));
    for segment in s.split(',') {
        let mut parts = segment.trim().split('=');
        let (Some(beat), Some(combo)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(beat), Some(combo)) = (beat.trim().parse::<f32>(), parse_itg_int(combo)) else {
            continue;
        };
        let miss_combo = parts.next().and_then(parse_itg_int).unwrap_or(combo);
        if beat.is_finite() {
            out.push(make(beat, combo.max(0) as u32, miss_combo.max(0) as u32));
        }
    }

    if out.is_empty() {
        return vec![make(0.0, 1, 1)];
    }

    out.sort_by(|a, b| {
        beat_to_note_row(beat(a))
            .cmp(&beat_to_note_row(beat(b)))
            .then_with(|| beat(a).total_cmp(&beat(b)))
    });
    dedup_last_by_row(&mut out, &beat);
    if out
        .first()
        .is_none_or(|segment| beat_to_note_row(beat(segment)) > 0)
    {
        out.insert(0, make(0.0, 1, 1));
    }
    out
}
