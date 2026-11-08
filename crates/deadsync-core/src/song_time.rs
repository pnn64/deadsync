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
    let nanos = f64::from(seconds) * SONG_TIME_NS_PER_SECOND;
    // This product has at most 45 significant bits, so a signed half cannot
    // round a sub-half value up to one. At 2^52, f64 values are integral.
    let rounded = if nanos.abs() < 4_503_599_627_370_496.0 {
        nanos + 0.5f64.copysign(nanos)
    } else {
        nanos
    };
    // The float-to-integer cast already saturates at the i64 bounds.
    rounded as SongTimeNs
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
    // Positive finite f32 bit patterns occupy 1..0x7f80_0000.
    if seconds_per_second.to_bits().wrapping_sub(1) < 0x7f7f_ffff {
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
    // Ordinary host deltas fit i64 and convert with one scalar instruction.
    let delta = if let Ok(nanos) = i64::try_from(delta_host_nanos) {
        nanos as f64
    } else {
        delta_host_nanos as f64
    };
    let scaled = delta * f64::from(slope);
    let magnitude = scaled.abs();
    if magnitude < 0.5 {
        0
    } else if magnitude < 4_503_599_627_370_496.0 {
        // A sub-half guard prevents a value just below 0.5 from biasing to 1.
        i128::from((scaled + 0.5f64.copysign(scaled)) as i64)
    } else {
        // Every f64 at or above 2^52 is integral; the cast also saturates.
        scaled as i128
    }
}

#[inline(always)]
#[must_use]
pub fn scaled_song_time_ns(seconds: f32, seconds_per_second: f32) -> SongTimeNs {
    song_time_ns_from_seconds(seconds * normalized_song_rate(seconds_per_second))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_differences_preserve_all_integer_bits_before_conversion() {
        let check = |lhs: i64, rhs: i64| {
            let expected =
                ((i128::from(lhs) - i128::from(rhs)) as f64 / SONG_TIME_NS_PER_SECOND) as f32;
            assert_eq!(
                song_time_ns_delta_seconds(lhs, rhs).to_bits(),
                expected.to_bits(),
                "{lhs} - {rhs}"
            );
        };
        let mut values = vec![i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX];
        for bit in 0..63 {
            for offset in -2..=2 {
                if let Some(value) = (1_i64 << bit).checked_add(offset) {
                    values.push(value);
                    values.push(-value);
                }
            }
        }
        for &lhs in &values {
            for &rhs in &values {
                check(lhs, rhs);
            }
        }
        let mut state = 0x1234_5678_9abc_def0_u64;
        for _ in 0..100_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let lhs = state as i64;
            state = state.wrapping_mul(0xd134_2543_de82_ef95);
            check(lhs, state as i64);
        }
    }

    #[test]
    fn seconds_round_trip_through_integer_nanoseconds() {
        let time = song_time_ns_from_seconds(12.345_678);
        assert!((song_time_ns_to_seconds(time) - 12.345_678).abs() <= 0.000_001);
    }

    #[test]
    fn invalid_seconds_stay_invalid() {
        for seconds in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(song_time_ns_from_seconds(seconds), INVALID_SONG_TIME_NS);
        }
        assert!(song_time_ns_invalid(INVALID_SONG_TIME_NS));
        assert!(song_time_ns_to_seconds(INVALID_SONG_TIME_NS).is_nan());
    }

    #[test]
    fn seconds_conversion_preserves_rounding_and_saturation() {
        for (seconds, expected) in [
            (0.0, 0),
            (-0.0, 0),
            (f32::from_bits(1), 0),
            (-f32::from_bits(1), 0),
            (1e-9, 1),
            (-1e-9, -1),
            (0.25, 250_000_000),
            (-0.25, -250_000_000),
            (8_192.0, 8_192_000_000_000),
            // Adjacent f32 values straddle the i64 nanosecond limits.
            (f32::from_bits(0x5009_705f), 9_223_371_776_000_000_000),
            (f32::from_bits(0x5009_7060), i64::MAX),
            (f32::from_bits(0xd009_705f), -9_223_371_776_000_000_000),
            (f32::from_bits(0xd009_7060), i64::MIN),
            (f32::MAX, i64::MAX),
            (-f32::MAX, i64::MIN),
        ] {
            assert_eq!(song_time_ns_from_seconds(seconds), expected, "{seconds:?}");
        }
    }

    #[test]
    fn scaled_time_uses_valid_positive_rates_only() {
        assert_eq!(scaled_song_time_ns(1.0, 1.5), 1_500_000_000);
        assert_eq!(scaled_song_time_ns(1.0, 0.0), 1_000_000_000);
        assert_eq!(scaled_song_delta_ns(10_000_000, 2.0), 20_000_000);
    }
}
