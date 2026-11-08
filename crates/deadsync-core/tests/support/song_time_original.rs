//! Frozen production reference from 3ba0ad8e6.
pub type SongTimeNs = i64;

pub const INVALID_SONG_TIME_NS: SongTimeNs = i64::MIN;
const SONG_TIME_NS_PER_SECOND: f64 = 1_000_000_000.0;
const MIN_VALID_SONG_TIME_NS: i128 = (i64::MIN + 1) as i128;

#[inline(always)]
#[must_use]
pub const fn song_time_ns_invalid(time_ns: SongTimeNs) -> bool {
    time_ns == INVALID_SONG_TIME_NS
}

#[inline(always)]
#[must_use]
pub fn song_time_ns_from_seconds(seconds: f32) -> SongTimeNs {
    if !seconds.is_finite() {
        return INVALID_SONG_TIME_NS;
    }
    // The float-to-integer cast already saturates at the i64 bounds.
    (f64::from(seconds) * SONG_TIME_NS_PER_SECOND).round() as SongTimeNs
}

#[inline(always)]
#[must_use]
pub fn song_time_ns_to_seconds(time_ns: SongTimeNs) -> f32 {
    if song_time_ns_invalid(time_ns) {
        return f32::NAN;
    }
    (time_ns as f64 / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
#[must_use]
pub fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    // The full difference fits u64, including MIN..MAX. Convert its magnitude
    // once, then restore the sign without rounding the timestamps separately.
    let magnitude = lhs.abs_diff(rhs) as f64;
    let delta = if lhs < rhs { -magnitude } else { magnitude };
    (delta / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
#[must_use]
pub fn song_time_ns_add_seconds(time_ns: SongTimeNs, delta_seconds: f32) -> SongTimeNs {
    if song_time_ns_invalid(time_ns) {
        return INVALID_SONG_TIME_NS;
    }
    let delta_ns = song_time_ns_from_seconds(delta_seconds);
    if song_time_ns_invalid(delta_ns) {
        return INVALID_SONG_TIME_NS;
    }
    time_ns.saturating_add(delta_ns)
}

#[inline(always)]
#[must_use]
pub fn normalized_song_rate(seconds_per_second: f32) -> f32 {
    if seconds_per_second.is_finite() && seconds_per_second > 0.0 {
        seconds_per_second
    } else {
        1.0
    }
}

#[inline(always)]
#[must_use]
pub fn song_time_ns_span_seconds(span_ns: i128) -> f32 {
    (span_ns as f64 / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
#[must_use]
pub fn clamp_song_time_ns(value: i128) -> SongTimeNs {
    value.clamp(MIN_VALID_SONG_TIME_NS, i128::from(i64::MAX)) as SongTimeNs
}

#[inline(always)]
#[must_use]
pub fn scaled_song_delta_ns(delta_host_nanos: i128, seconds_per_second: f32) -> i128 {
    let slope = normalized_song_rate(seconds_per_second);
    (delta_host_nanos as f64 * f64::from(slope)).round() as i128
}

#[inline(always)]
#[must_use]
pub fn scaled_song_time_ns(seconds: f32, seconds_per_second: f32) -> SongTimeNs {
    song_time_ns_from_seconds(seconds * normalized_song_rate(seconds_per_second))
}
