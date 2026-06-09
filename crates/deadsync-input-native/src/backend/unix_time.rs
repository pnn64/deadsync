use std::time::{Duration, Instant};

const EVENT_STALE_TOLERANCE_NS: u64 = 5_000_000_000;
const EVENT_FUTURE_TOLERANCE_NS: u64 = 50_000_000;

#[derive(Clone, Copy, Debug)]
pub struct EventTimeSample {
    pub instant: Instant,
    pub host_nanos: u64,
    pub clock_nanos: Option<u64>,
}

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
        return Some((
            sample.instant.checked_add(Duration::from_nanos(delta))?,
            sample.host_nanos.saturating_add(delta),
        ));
    }
    let delta = sample_clock_nanos - event_clock_nanos;
    Some((
        sample.instant.checked_sub(Duration::from_nanos(delta))?,
        sample.host_nanos.saturating_sub(delta),
    ))
}

#[inline(always)]
pub fn event_time(sample: EventTimeSample, sec: i64, usec: i64) -> (Instant, u64) {
    let Some(sample_clock_nanos) = sample.clock_nanos else {
        return (sample.instant, sample.host_nanos);
    };
    let Some(event_clock_nanos) = event_clock_nanos(sec, usec) else {
        return (sample.instant, sample.host_nanos);
    };
    if event_clock_nanos > sample_clock_nanos.saturating_add(EVENT_FUTURE_TOLERANCE_NS)
        || sample_clock_nanos.saturating_sub(event_clock_nanos) > EVENT_STALE_TOLERANCE_NS
    {
        return (sample.instant, sample.host_nanos);
    }
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
}
