pub type SongTimeNs = i64;

pub const INVALID_SONG_TIME_NS: SongTimeNs = i64::MIN;
const SONG_TIME_NS_PER_SECOND: f64 = 1_000_000_000.0;
const MIN_VALID_SONG_TIME_NS: i128 = (i64::MIN + 1) as i128;

#[inline(always)]
pub const fn song_time_ns_invalid(time_ns: SongTimeNs) -> bool {
    time_ns == INVALID_SONG_TIME_NS
}

#[inline(always)]
pub fn song_time_ns_from_seconds(seconds: f32) -> SongTimeNs {
    if !seconds.is_finite() {
        return INVALID_SONG_TIME_NS;
    }
    let nanos = (seconds as f64 * SONG_TIME_NS_PER_SECOND).round();
    nanos.clamp((i64::MIN + 1) as f64, i64::MAX as f64) as SongTimeNs
}

#[inline(always)]
pub fn song_time_ns_to_seconds(time_ns: SongTimeNs) -> f32 {
    if song_time_ns_invalid(time_ns) {
        return f32::NAN;
    }
    (time_ns as f64 / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
pub fn song_time_ns_delta_seconds(lhs: SongTimeNs, rhs: SongTimeNs) -> f32 {
    ((lhs as i128 - rhs as i128) as f64 / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
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
pub fn normalized_song_rate(seconds_per_second: f32) -> f32 {
    if seconds_per_second.is_finite() && seconds_per_second > 0.0 {
        seconds_per_second
    } else {
        1.0
    }
}

#[inline(always)]
pub fn song_time_ns_span_seconds(span_ns: i128) -> f32 {
    (span_ns as f64 / SONG_TIME_NS_PER_SECOND) as f32
}

#[inline(always)]
pub fn clamp_song_time_ns(value: i128) -> SongTimeNs {
    value.clamp(MIN_VALID_SONG_TIME_NS, i64::MAX as i128) as SongTimeNs
}

#[inline(always)]
pub fn scaled_song_delta_ns(delta_host_nanos: i128, seconds_per_second: f32) -> i128 {
    let slope = normalized_song_rate(seconds_per_second);
    (delta_host_nanos as f64 * slope as f64).round() as i128
}

#[inline(always)]
pub fn scaled_song_time_ns(seconds: f32, seconds_per_second: f32) -> SongTimeNs {
    song_time_ns_from_seconds(seconds * normalized_song_rate(seconds_per_second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_round_trip_through_integer_nanoseconds() {
        let time = song_time_ns_from_seconds(12.345_678);
        assert!((song_time_ns_to_seconds(time) - 12.345_678).abs() <= 0.000_001);
    }

    #[test]
    fn invalid_seconds_stay_invalid() {
        assert_eq!(song_time_ns_from_seconds(f32::NAN), INVALID_SONG_TIME_NS);
        assert!(song_time_ns_invalid(INVALID_SONG_TIME_NS));
        assert!(song_time_ns_to_seconds(INVALID_SONG_TIME_NS).is_nan());
    }

    #[test]
    fn scaled_time_uses_valid_positive_rates_only() {
        assert_eq!(scaled_song_time_ns(1.0, 1.5), 1_500_000_000);
        assert_eq!(scaled_song_time_ns(1.0, 0.0), 1_000_000_000);
        assert_eq!(scaled_song_delta_ns(10_000_000, 2.0), 20_000_000);
    }
}
