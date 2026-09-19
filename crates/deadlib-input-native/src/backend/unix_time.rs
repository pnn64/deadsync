use std::time::{Duration, Instant};

const EVENT_STALE_TOLERANCE_NS: u64 = 5_000_000_000;
const EVENT_FUTURE_TOLERANCE_NS: u64 = 50_000_000;

#[derive(Clone, Copy, Debug)]
pub struct EventTimeSample {
    pub instant: Instant,
    pub host_nanos: u64,
    pub clock_nanos: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct EventTimeCache {
    sec: i64,
    usec: i64,
    mapped: Option<(Instant, u64)>,
}

impl EventTimeCache {
    #[inline(always)]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sec: 0,
            usec: 0,
            mapped: None,
        }
    }

    #[inline(always)]
    pub fn event_time(&mut self, sample: EventTimeSample, sec: i64, usec: i64) -> (Instant, u64) {
        if self.sec == sec
            && self.usec == usec
            && let Some(mapped) = self.mapped
        {
            return mapped;
        }
        let mapped = event_time(sample, sec, usec);
        self.sec = sec;
        self.usec = usec;
        self.mapped = Some(mapped);
        mapped
    }
}

impl Default for EventTimeCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
#[inline(always)]
fn monotonic_nanos_now() -> Option<u64> {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `clock_gettime` writes into the provided stack `timespec`, and
    // `CLOCK_MONOTONIC` is a valid clock id on supported Unix targets.
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    if rc < 0 || ts.tv_sec < 0 || ts.tv_nsec < 0 {
        return None;
    }
    Some((ts.tv_sec as u64).saturating_mul(1_000_000_000) + ts.tv_nsec as u64)
}

#[cfg(unix)]
#[inline(always)]
pub fn receipt_time(instant_nanos: impl FnOnce(Instant) -> u64) -> EventTimeSample {
    let instant = Instant::now();
    EventTimeSample {
        instant,
        host_nanos: instant_nanos(instant),
        clock_nanos: monotonic_nanos_now(),
    }
}

#[inline(always)]
fn event_clock_nanos(sec: i64, usec: i64) -> Option<u64> {
    if sec < 0 || !(0..1_000_000).contains(&usec) {
        return None;
    }
    Some((sec as u64).saturating_mul(1_000_000_000) + (usec as u64).saturating_mul(1_000))
}

#[inline(always)]
fn map_event_time(
    sample: EventTimeSample,
    event_clock_nanos: u64,
    sample_clock_nanos: u64,
) -> Option<(Instant, u64)> {
    if event_clock_nanos >= sample_clock_nanos {
        let delta = event_clock_nanos - sample_clock_nanos;
        if delta > EVENT_FUTURE_TOLERANCE_NS {
            return None;
        }
        return Some((
            sample.instant.checked_add(Duration::from_nanos(delta))?,
            sample.host_nanos.saturating_add(delta),
        ));
    }
    let delta = sample_clock_nanos - event_clock_nanos;
    if delta > EVENT_STALE_TOLERANCE_NS {
        return None;
    }
    Some((
        sample.instant.checked_sub(Duration::from_nanos(delta))?,
        sample.host_nanos.saturating_sub(delta),
    ))
}

#[inline(always)]
#[must_use]
pub fn event_time(sample: EventTimeSample, sec: i64, usec: i64) -> (Instant, u64) {
    let Some(sample_clock_nanos) = sample.clock_nanos else {
        return (sample.instant, sample.host_nanos);
    };
    let Some(event_clock_nanos) = event_clock_nanos(sec, usec) else {
        return (sample.instant, sample.host_nanos);
    };
    map_event_time(sample, event_clock_nanos, sample_clock_nanos)
        .unwrap_or((sample.instant, sample.host_nanos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_time_maps_earlier_kernel_timestamp() {
        let base = Instant::now();
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 9_000_000_000,
            clock_nanos: Some(42_000_000_000),
        };
        let (timestamp, host_nanos) = event_time(sample, 41, 997_500);
        assert_eq!(host_nanos, 8_997_500_000);
        assert_eq!(timestamp, base - Duration::from_micros(2_500));
    }

    #[test]
    fn event_time_maps_small_future_kernel_timestamp() {
        let base = Instant::now();
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 5_000_000,
            clock_nanos: Some(100_000_000),
        };
        let (timestamp, host_nanos) = event_time(sample, 0, 100_800);
        assert_eq!(host_nanos, 5_800_000);
        assert_eq!(timestamp, base + Duration::from_micros(800));
    }

    #[test]
    fn event_time_falls_back_when_kernel_time_is_implausible() {
        let base = Instant::now();
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 100,
            clock_nanos: Some(5_000_000_000),
        };
        let (timestamp, host_nanos) = event_time(sample, 15, 0);
        assert_eq!(host_nanos, 100);
        assert_eq!(timestamp, base);
    }

    #[test]
    fn event_time_preserves_tolerance_boundaries() {
        let base = Instant::now();
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 9_000_000_000,
            clock_nanos: Some(42_000_000_000),
        };
        for (sec, usec, expected_delta_us) in [
            (36, 999_999, 0), // More than five seconds stale: receipt time.
            (37, 0, -5_000_000),
            (37, 1, -4_999_999),
            (41, 999_999, -1),
            (42, 0, 0),
            (42, 1, 1),
            (42, 49_999, 49_999),
            (42, 50_000, 50_000),
            (42, 50_001, 0), // More than 50 milliseconds future: receipt time.
            (-1, 0, 0),
            (42, -1, 0),
            (42, 1_000_000, 0),
        ] {
            let expected_delta_us: i64 = expected_delta_us;
            let duration = Duration::from_micros(expected_delta_us.unsigned_abs());
            let expected_timestamp = if expected_delta_us < 0 {
                base - duration
            } else {
                base + duration
            };
            let expected_host_nanos =
                (i128::from(sample.host_nanos) + i128::from(expected_delta_us) * 1_000) as u64;
            assert_eq!(
                event_time(sample, sec, usec),
                (expected_timestamp, expected_host_nanos),
                "kernel timestamp {sec}:{usec}"
            );
        }
    }

    #[test]
    fn event_time_preserves_saturating_clock_boundaries() {
        let base = Instant::now();
        let event_clock = u64::MAX / 1_000 * 1_000;
        let sec = (event_clock / 1_000_000_000) as i64;
        let usec = (event_clock % 1_000_000_000 / 1_000) as i64;
        let sample = EventTimeSample {
            instant: base,
            host_nanos: u64::MAX - 5_000_000,
            clock_nanos: Some(event_clock - 20_000_000),
        };
        // The old future bound saturated at u64::MAX; this timestamp still fits.
        assert_eq!(
            event_time(sample, sec, usec),
            (base + Duration::from_millis(20), u64::MAX)
        );
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 100,
            clock_nanos: Some(u64::MAX),
        };
        assert_eq!(
            event_time(sample, sec, usec),
            (base - Duration::from_nanos(u64::MAX - event_clock), 0)
        );
    }

    #[test]
    fn event_time_cache_matches_uncached_mapping() {
        let base = Instant::now();
        let sample = EventTimeSample {
            instant: base,
            host_nanos: 9_000_000_000,
            clock_nanos: Some(42_000_000_000),
        };
        let mut cache = EventTimeCache::new();

        for (sec, usec) in [
            (41, 997_500),
            (41, 997_500),
            (42, 1_000),
            (42, 1_000),
            (-1, 0),
            (-1, 0),
            (42, 1_000_000),
            (42, 1_000_000),
        ] {
            assert_eq!(
                cache.event_time(sample, sec, usec),
                event_time(sample, sec, usec)
            );
        }
    }
}
